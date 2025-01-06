use aws_config::{imds::client, BehaviorVersion};
use aws_sdk_s3::Client;
use clap::{Parser, Subcommand};
use eyre::{eyre, ContextCompat, Ok, WrapErr};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

fn skypath() -> eyre::Result<PathBuf> {
    Ok(Path::new(&std::env::var("HOME").wrap_err("Can not read HOME env var")?).join(".sky"))
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Build,
    Deploy,
}

struct Crate {
    name: String,
    resources: Vec<Resource>,
}

/// Return crate info from Cargo.toml
fn crat() -> eyre::Result<Crate> {
    let path = std::env::current_dir().wrap_err("Failed to get current dir")?;
    let cargo_toml: toml::Value = cargotoml(&path)?;

    Ok(Crate {
        name: cargo_toml
            .get("package")
            .and_then(|pkg| pkg.get("name"))
            .and_then(|name| name.as_str())
            .wrap_err("Failed to get crate name from Cargo.toml")?
            .to_string(),

        resources: resources(&path)?,
    })
}

/// Return the list of dirs with functions to deploy
fn functions() -> eyre::Result<Vec<PathBuf>> {
    let mut result = vec![];
    let project = crat()?;

    for entry in std::fs::read_dir(
        &skypath()
            .wrap_err("Failed to resolve sky path")?
            .join(project.name),
    )
    .wrap_err("Failed to read dir")?
    {
        let path = entry.wrap_err("Failed to get dir entry")?.path();

        if path.is_dir() {
            result.push(path);
        }
    }

    Ok(result)
}

#[derive(Debug)]
struct Queue {
    name: String,
    concurrency: u32,
}

#[derive(Debug)]
struct KvDb {
    name: String,
}

#[derive(Debug)]
struct SqlDb {
    name: String,
}

#[derive(Debug)]
enum Resource {
    Queue(Queue),
    KvDb(KvDb),
    SqlDb(SqlDb),
}

/// The hash with all the resources specific to the function
fn resources(path: &PathBuf) -> eyre::Result<Vec<Resource>> {
    let mut result = vec![];
    let src_path = Path::new(path);
    let cargo_toml_path = src_path.join("Cargo.toml");

    let cargo_toml: toml::Value = std::fs::read_to_string(cargo_toml_path)
        .wrap_err("Failed to read Cargo.toml: {cargo_toml_path:?}")?
        .parse::<toml::Value>()
        .wrap_err("Failed to parse Cargo.toml")?;

    for category_name in vec!["kvdb", "queue"] {
        let category = cargo_toml
            .get("package")
            .wrap_err("No [package]")?
            .get("metadata")
            .wrap_err("No [metadata]")?
            .get("sky")
            .wrap_err("No [sky]")?
            .get(category_name);

        if category.is_none() {
            continue;
        }

        let category = category
            .wrap_err(format!("No category {category_name} found"))?
            .as_table()
            .wrap_err("Section format is wrong")?;

        for resource_name in category.keys() {
            let resource = category
                .get(resource_name)
                .wrap_err("No {resource_name}")?
                .clone();

            result.push(match category_name {
                "queue" => Resource::Queue(Queue {
                    name: resource_name.clone(),

                    concurrency: resource
                        .get("concurrency")
                        .unwrap_or(&toml::Value::Integer(1))
                        .as_integer()
                        .unwrap() as u32,
                }),

                "kvdb" => Resource::KvDb(KvDb {
                    name: resource_name.clone(),
                }),

                _ => unreachable!(),
            });
        }
    }

    Ok(result)
}

/// Policy statements to allow a function to access a resource
///
/// Current all functions in a crate have access to all resources.
fn policies(crat: &Crate) -> String {
    let mut template = String::default();

    for resource in crat.resources.iter() {
        template.push_str(&match resource {
            Resource::KvDb(kvdb) => format!(
                "
            - PolicyName: DynamoPolicy{}
              PolicyDocument:
                  Version: '2012-10-17'
                  Statement:
                  - Effect: Allow
                    Action:
                      - dynamodb:BatchGetItem
                      - dynamodb:BatchWriteItem
                      - dynamodb:ConditionCheckItem
                      - dynamodb:PutItem
                      - dynamodb:DescribeTable
                      - dynamodb:DeleteItem
                      - dynamodb:GetItem
                      - dynamodb:Scan
                      - dynamodb:Query
                      - dynamodb:UpdateItem
                    Resource: !GetAtt
                      - DynamoDBTable{}{}
                      - Arn",
                kvdb.name, crat.name, kvdb.name,
            ),

            Resource::SqlDb(_) => format!(""),
            _ => format!(""),
        })
    }

    template
}

/// CFN template for a function — a function itself and its role
fn endpoint2template(name: &str, crat: &Crate) -> String {
    let policies = policies(&crat);

    format!(
        "
        Endpoint{name}:
          Type: AWS::Lambda::Function
          Properties:
                FunctionName: {name}
                Handler: bootstrap
                Runtime: provided.al2023
                Role:
                  Fn::GetAtt:
                    - EndpointRole{name}
                    - Arn
                MemorySize: 1024
                Code:
                  S3Bucket: my-lambda-function-code-test
                  S3Key: bootstrap.zip
        EndpointRole{name}:
          Type: AWS::IAM::Role
          Properties:
            AssumeRolePolicyDocument:
              Version: '2012-10-17'
              Statement:
              - Effect: Allow
                Principal:
                  Service:
                  - lambda.amazonaws.com
                Action:
                - sts:AssumeRole
            Path: \"/\"
            Policies:
            - PolicyName: AppendToLogsPolicy
              PolicyDocument:
                Version: '2012-10-17'
                Statement:
                - Effect: Allow
                  Action:
                  - logs:CreateLogGroup
                  - logs:CreateLogStream
                  - logs:PutLogEvents
                  Resource: \"*\"
            {policies}
        EndpointUrl{name}:
            Type: AWS::Lambda::Url
            Properties:
                AuthType: NONE
                TargetFunctionArn: !Ref Endpoint{name}
        EndpointUrlPermission{name}:
            Type: AWS::Lambda::Permission
            Properties:
                Action: lambda:InvokeFunctionUrl
                FunctionUrlAuthType: 'NONE'
                FunctionName: !Ref Endpoint{name}
                Principal: \"*\"
        "
    )
}

fn worker2template(name: &str, resources: &Vec<Resource>, crat: &Crate) -> eyre::Result<String> {
    let policies = policies(&crat);

    let queue = resources
        .iter()
        .find_map(|r| match r {
            Resource::Queue(queue) => Some(queue),
            _ => None,
        })
        .wrap_err("No queue resource found in Cargo.toml")?;

    Ok(format!(
        "
        Worker{name}:
          Type: AWS::Lambda::Function
          Properties:
                FunctionName: {name}
                Handler: bootstrap
                Runtime: provided.al2023
                Role:
                  Fn::GetAtt:
                    - WorkerRole{name}
                    - Arn
                MemorySize: 1024
                Code:
                  S3Bucket: my-lambda-function-code-test
                  S3Key: bootstrap.zip

        WorkerRole{name}:
          Type: AWS::IAM::Role
          Properties:
            AssumeRolePolicyDocument:
              Version: '2012-10-17'
              Statement:
              - Effect: Allow
                Principal:
                  Service:
                  - lambda.amazonaws.com
                Action:
                - sts:AssumeRole
            Path: \"/\"
            Policies:
            - PolicyName: AppendToLogsPolicy
              PolicyDocument:
                Version: '2012-10-17'
                Statement:
                - Effect: Allow
                  Action:
                  - logs:CreateLogGroup
                  - logs:CreateLogStream
                  - logs:PutLogEvents
                  Resource: \"*\"
            - PolicyName: QueuePolicy
              PolicyDocument:
                Version: '2012-10-17'
                Statement:
                - Effect: Allow
                  Action:
                  - sqs:ChangeMessageVisibility
                  - sqs:DeleteMessage
                  - sqs:GetQueueAttributes
                  - sqs:GetQueueUrl
                  - sqs:ReceiveMessage
                  Resource:
                    Fn::GetAtt:
                      - WorkerQueue{name}
                      - Arn
            {policies}

        WorkerQueue{name}:
            Type: AWS::SQS::Queue
            Properties:
                QueueName: WorkerQueue{}
                VisibilityTimeout: 60
                MaximumMessageSize: 2048
                MessageRetentionPeriod: 345600
                ReceiveMessageWaitTimeSeconds: 20

        WorkerQueueEventSourceMapping{name}:
            Type: AWS::Lambda::EventSourceMapping
            Properties:
                EventSourceArn:
                    Fn::GetAtt:
                    - WorkerQueue{name}
                    - Arn
                FunctionName:
                    Ref: Worker{name}
                ScalingConfig:
                    MaximumConcurrency: {}
        ",
        queue.name, queue.concurrency,
    ))
}

/// CFN template for a resource (e.g. a queue or a db)
fn resource2template(name: &str, resource: &Resource) -> String {
    match resource {
        Resource::KvDb(kvdb) => {
            format!(
                "
        DynamoDBTable{}{}:
            Type: AWS::DynamoDB::Table
            Properties:
                TableName: {}
                AttributeDefinitions:
                    - AttributeName: id
                      AttributeType: S
                KeySchema:
                    - AttributeName: id
                      KeyType: HASH
                ProvisionedThroughput:
                    ReadCapacityUnits: 5
                    WriteCapacityUnits: 5
            ",
                name, kvdb.name, kvdb.name,
            )
        }

        Resource::SqlDb(_sqldb) => format!("KVDB"),

        Resource::Queue(queue) => format!(
            "
            WorkerQueue{name}:
                Type: AWS::SQS::Queue
                Properties:
                    QueueName: WorkerQueue{}
                    VisibilityTimeout: 60
                    MaximumMessageSize: 2048
                    MessageRetentionPeriod: 345600
                    ReceiveMessageWaitTimeSeconds: 20
            WorkerQueueEventSourceMapping{name}:
                Type: 'AWS::Lambda::EventSourceMapping'
                Properties:
                    EventSourceArn:
                        Fn::GetAtt:
                        - WorkerQueue{name}
                        - Arn
                    FunctionName:
                        Ref: LambdaFunction{name}:
            ",
            queue.name,
        ),
    }
}

/// Build all assets and CFN templates
fn build() -> eyre::Result<()> {
    let project = crat()?;
    println!("Building \"{}\"...", project.name);

    for path in functions()? {
        let status = std::process::Command::new("cargo")
            .arg("lambda")
            .arg("build")
            .current_dir(&path)
            .output()
            .expect("Failed to execute process")
            .status;

        if !status.success() {
            panic!("Build failed: {:?}, {}", path, status.code().unwrap());
        }
    }

    println!("Done!");
    Ok(())
}

/// Read Cargo.toml in a given dir
fn cargotoml(path: &Path) -> eyre::Result<toml::Value> {
    std::fs::read_to_string(path.join("Cargo.toml"))
        .wrap_err("Failed to read Cargo.toml: {cargo_toml_path:?}")?
        .parse::<toml::Value>()
        .wrap_err("Failed to parse Cargo.toml")
}

/// Generate CFN template for all functions
fn template(crat: &Crate, functions: Vec<PathBuf>) -> eyre::Result<String> {
    let mut template = "Resources:".to_string();

    // Define global resources from the app's Cargo.toml, e.g. a DB
    for resource in crat.resources.iter() {
        template.push_str(&resource2template(&crat.name, &resource));
        template.push_str("\n");
    }

    for path in functions {
        let cargo_toml: toml::Value = cargotoml(&path)?;

        let meta = cargo_toml
            .get("package")
            .wrap_err("No [package]")?
            .get("metadata")
            .wrap_err("No [metadata]")?
            .get("sky")
            .wrap_err("No [sky]")?
            .get("function")
            .wrap_err("No [function]")?;

        let name = meta
            .get("name")
            .wrap_err("No [name]")?
            .as_str()
            .wrap_err("Not a string")?;

        let role = meta
            .get("role")
            .wrap_err("No [role]")?
            .as_str()
            .wrap_err("Not a string")?;

        let resources = resources(&path)?;

        if role == "endpoint" {
            template.push_str(&endpoint2template(name, &crat));
        }

        if role == "worker" {
            template.push_str(
                &worker2template(name, &resources, &crat)
                    .wrap_err("Failed to build worker template")?,
            );
        }

        template.push_str("\n");
    }

    Ok(template)
}

/// Bundle assets and upload to S3, assuming all functions are built
fn bundle(functions: &Vec<PathBuf>) -> eyre::Result<()> {
    for path in functions {
        println!("Building {path:?} with cargo-lambda...");

        let status = std::process::Command::new("cargo")
            .arg("lambda")
            .arg("build")
            .arg("--release")
            .current_dir(&path)
            .output()
            .wrap_err("Failed to execute the process")?
            .status;

        if !status.success() {
            Err(eyre!("Build failed: {path:?} {:?}", status.code()))?;
        }

        println!("Bundling {path:?}...");
        let file = std::fs::File::create(&path.join("bootstrap.zip"))?;
        let mut zip = zip::ZipWriter::new(file);

        let mut f = std::fs::File::open(
            path.join("target")
                .join("lambda")
                .join("bootstrap")
                .join("bootstrap")
                .to_str()
                .ok_or(eyre!("Failed to construct asset path"))?,
        )?;

        zip.start_file("bootstrap", SimpleFileOptions::default())?;
        std::io::copy(&mut f, &mut zip)?;
        zip.finish()?;
    }

    Ok(())
}

/// All bundled assets to S3
async fn upload(functions: &Vec<PathBuf>) -> eyre::Result<()> {
    for path in functions {
        let bucket_name = "my-lambda-function-code-test";
        let key = "bootstrap.zip";
        let body = aws_sdk_s3::primitives::ByteStream::from_path(path.join("bootstrap.zip")).await;

        let config = aws_config::defaults(BehaviorVersion::v2024_03_28())
            .load()
            .await;

        let client = Client::new(&config);

        client
            .put_object()
            .bucket(bucket_name)
            .key(key)
            .body(body.unwrap())
            .send()
            .await
            .wrap_err("Failed to upload file to S3")?;
    }

    Ok(())
}

/// Check if the stack already exists
async fn is_exists(client: &aws_sdk_cloudformation::Client, name: &str) -> eyre::Result<bool> {
    let result = client
        .describe_stacks()
        .set_stack_name(Some(name.into()))
        .send()
        .await;

    if let Err(e) = &result {
        if let aws_sdk_cloudformation::error::SdkError::ServiceError(err) = e {
            if err.err().meta().code().unwrap().eq("ValidationError") {
                return Ok(false);
            } else {
                return Err(eyre::eyre!(
                    "Service error while describing stack: {:?}",
                    err
                ));
            }
        } else {
            return Err(eyre::eyre!("Failed to describe stack: {:?}", e));
        }
    }

    Ok(true)
}

/// Provision cloud resources using CFN template
async fn provision(template: &str) -> eyre::Result<()> {
    let config = aws_config::defaults(BehaviorVersion::v2024_03_28())
        .load()
        .await;

    let client = aws_sdk_cloudformation::Client::new(&config);
    let name = "sky-example";
    let capabilities = aws_sdk_cloudformation::types::Capability::CapabilityIam;

    if is_exists(&client, name).await? {
        client
            .update_stack()
            .capabilities(capabilities)
            .stack_name(name)
            .template_body(template)
            .send()
            .await
            .wrap_err("Failed to create stack")?;
    } else {
        client
            .create_stack()
            .capabilities(capabilities)
            .stack_name(name)
            .template_body(template)
            .send()
            .await
            .wrap_err("Failed to create stack")?;
    }

    Ok(())
}

/// Build and deploy all assets using CFN template
async fn deploy() -> eyre::Result<()> {
    let crat = crat().unwrap();
    let functions = functions().wrap_err("Failed to bundle assets")?;
    println!("Deploying \"{}\"...", crat.name);
    bundle(&functions)?;
    upload(&functions).await?;
    let template = template(&crat, functions)?;
    println!("Provisioning resources:\n{template}");
    provision(&template).await?;
    println!("Done!");
    Ok(())
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Build) => {
            if let Err(error) = build() {
                println!("{error}");
                return;
            }
        }
        Some(Commands::Deploy) => {
            if let Err(error) = build() {
                println!("{error:?}");
                return;
            }

            if let Err(error) = deploy().await {
                println!("{error:?}");
                return;
            }
        }
        None => {}
    }
}
