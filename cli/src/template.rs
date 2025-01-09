use crate::function::Function;
use crate::{Crate, Resource};
use eyre::{ContextCompat, Ok, WrapErr};

#[derive(Clone)]
pub struct Template {
    crat: Crate,
    functions: Vec<Function>,
    pub template: String,
}

impl Template {
    /// CFN template for a resource (e.g. a queue or a db)
    fn resource2template(&self, resource: &Resource) -> String {
        let name = self.crat.name.clone();

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

    pub fn new(crat: &Crate, functions: Vec<Function>) -> eyre::Result<Self> {
        let mut template = Template {
            crat: crat.clone(),
            template: "Resources:".to_string(),
            functions,
        };

        // Define global resources from the app's Cargo.toml, e.g. a DB
        for resource in crat.resources.iter() {
            template
                .template
                .push_str(&template.resource2template(&resource));
            template.template.push_str("\n");
        }

        for function in template.functions.clone() {
            if function.role()? == "endpoint" {
                template.template.push_str(&template.endpoint(&function)?);
            }

            if function.role()? == "worker" {
                template
                    .template
                    .push_str(&template.worker(&function).wrap_err("Failed to build worker template")?);
            }

            template.template.push_str("\n");
        }

        Ok(template)
    }

    /// Policy statements to allow a function to access a resource
    ///
    /// Current all functions in a crate have access to all resources.
    fn policies(&self) -> String {
        let mut template = String::default();

        for resource in self.crat.resources.iter() {
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
                    kvdb.name, self.crat.name, kvdb.name,
                ),

                Resource::SqlDb(_) => format!(""),
                _ => format!(""),
            })
        }

        template
    }

    /// CFN template for a REST endpoint function — a function itself and its role
    fn endpoint(&self, function: &Function) -> eyre::Result<String> {
        let policies = self.policies();
        let name = function.name()?;

        Ok(format!(
            "
            Endpoint{name}:
              Type: AWS::Lambda::Function
              Properties:
                FunctionName: {name}
                Handler: bootstrap
                Runtime: provided.al2023
                Environment:
                  Variables:
                    databaseName: lambdadb
                    databaseUser: admin
                Role:
                    Fn::GetAtt:
                    - EndpointRole{name}
                    - Arn
                MemorySize: 1024
                Code:
                    S3Bucket: my-lambda-function-code-test
                    S3Key: {}
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
            ",
            function
                .bundle_path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        ))
    }

    /// CFN template for a worker function
    fn worker(&self, function: &Function) -> eyre::Result<String> {
        let policies = self.policies();
        let name = function.name()?;

        let queue = function
            .resources()
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
                      S3Key: {}

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
            function
                .bundle_path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap(),
            queue.name,
            queue.concurrency,
        ))
    }
}
