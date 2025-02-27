use crate::crat::Crate;
use crate::function::Function;
use crate::secret::Secret;
use crate::Resource;
use aws_config::BehaviorVersion;
use eyre::{ContextCompat, Ok, WrapErr};
use toml::Value;

#[derive(Clone, Debug)]
pub struct Template {
    bucket: String,
    client: aws_sdk_cloudformation::Client,
    crat: Crate,
    functions: Vec<Function>,
    username_escaped: String,
    username: String,
    pub template: String,
}

impl Template {
    /// CFN template for a resource (e.g. a queue or a db)
    fn resource(&self, resource: &Resource) -> String {
        match resource {
            Resource::KvDb(kvdb) => format!(
                "
            DynamoDBTable{name}:
                Type: AWS::DynamoDB::Table
                Properties:
                    TableName: {db_name}
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
                name = self.prefixed(vec![&kvdb.name]),
                db_name = kvdb.name,
            ),

            _ => unimplemented!(),
        }
    }

    /// Domain and paths for endpoint lambdas
    fn routing(&self) -> String {
        let project_name = self.crat.name.clone();
        let functions: Vec<Function> = self.functions.clone();

        let functions = functions
            .iter()
            .filter(|f| f.role().unwrap() == "endpoint")
            .collect::<Vec<&Function>>();

        let default_origin_name = self.prefixed(vec![&functions[0].name().unwrap()]);

        let origins = functions
            .iter()
            .map(|f| {
                format!(
                    "
                        - Id: EndpointOrigin{name}
                          DomainName: !Select [2, !Split ['/', !GetAtt EndpointUrl{name}.FunctionUrl]]
                          CustomOriginConfig:
                            OriginProtocolPolicy: https-only
                    ",
                    name = self.prefixed(vec![&f.name().unwrap()])
                )
            })
            .collect::<Vec<String>>()
            .join("\n");

        let behaviors = functions
            .iter()
            .map(|f| {
                // Manage trailing slashes in the template
                let re = regex::Regex::new(r"/$").unwrap();

                format!(
                    "
                        - PathPattern: {path}
                          AllowedMethods:
                          - DELETE
                          - GET
                          - HEAD
                          - OPTIONS
                          - PATCH
                          - POST
                          - PUT
                          OriginRequestPolicyId: b689b0a8-53d0-40ab-baf2-68738e2966ac
                          CachePolicyId: 4135ea2d-6df8-44a3-9df3-4b5a84be39ad
                          ForwardedValues:
                              QueryString: true
                          TargetOriginId: EndpointOrigin{name}
                          ViewerProtocolPolicy: redirect-to-https
                          Compress: true
                        - PathPattern: {path}/
                          AllowedMethods:
                          - DELETE
                          - GET
                          - HEAD
                          - OPTIONS
                          - PATCH
                          - POST
                          - PUT
                          OriginRequestPolicyId: b689b0a8-53d0-40ab-baf2-68738e2966ac
                          CachePolicyId: 4135ea2d-6df8-44a3-9df3-4b5a84be39ad
                          ForwardedValues:
                              QueryString: true
                          TargetOriginId: EndpointOrigin{name}
                          ViewerProtocolPolicy: redirect-to-https
                          Compress: true
                    ",
                    path = re.replace_all(
                        &f.url_path().unwrap_or(f.name().unwrap().to_lowercase()),
                        ""
                    ),
                    name = self.prefixed(vec![&f.name().unwrap()])
                )
            })
            .collect::<Vec<String>>()
            .join("\n");

        format!(
            "
            EndpointDistribution{project_name}:
                Type: AWS::CloudFront::Distribution
                Properties:
                    DistributionConfig:
                        Aliases:
                        - {project_name}.usekinetics.com
                        Enabled: true
                        CacheBehaviors:
                        {behaviors}
                        DefaultCacheBehavior:
                            AllowedMethods:
                            - DELETE
                            - GET
                            - HEAD
                            - OPTIONS
                            - PATCH
                            - POST
                            - PUT
                            DefaultTTL: 0
                            MaxTTL: 0
                            MinTTL: 0
                            ForwardedValues:
                                QueryString: true
                                Headers:
                                - '*'
                                Cookies:
                                    Forward: all
                            TargetOriginId: EndpointOrigin{default_origin_name}
                            ViewerProtocolPolicy: allow-all
                            Compress: true
                        Origins:
                        {origins}
                        ViewerCertificate:
                            AcmCertificateArn: !Ref EndpointDistributionDomainCert{project_name}
                            SslSupportMethod: sni-only
                            MinimumProtocolVersion: TLSv1
            EndpointDistributionDomainCert{project_name}:
                Type: AWS::CertificateManager::Certificate
                Properties:
                    DomainName: {project_name}.usekinetics.com
                    ValidationMethod: DNS
                    DomainValidationOptions:
                    - DomainName: {project_name}.usekinetics.com
                      HostedZoneId: 'Z00296463IS4S0ZO4ABOR'
            EndpointDistributionAliasRecord{project_name}:
                Type: AWS::Route53::RecordSet
                Properties:
                    HostedZoneId: 'Z00296463IS4S0ZO4ABOR'
                    Name: '{project_name}.usekinetics.com.'
                    Type: 'A'
                    AliasTarget:
                        HostedZoneId: 'Z2FDTNDATAQYW2'  # CloudFront Hosted Zone ID
                        DNSName: !GetAtt EndpointDistribution{project_name}.DomainName
            "
        )
    }

    pub async fn new(
        crat: &Crate,
        functions: Vec<Function>,
        secrets: Vec<Secret>,
        bucket: &str,
        username_escaped: &str,
        username: &str,
    ) -> eyre::Result<Self> {
        let config = aws_config::defaults(BehaviorVersion::v2024_03_28())
            .load()
            .await;

        let client = aws_sdk_cloudformation::Client::new(&config);

        let mut template = Template {
            bucket: bucket.to_string(),
            client,
            crat: crat.clone(),
            template: "Resources:".to_string(),
            username_escaped: username_escaped.to_string(),
            username: username.to_string(),
            functions,
        };

        // Define global resources from the app's Cargo.toml, e.g. a DB
        for resource in crat.resources.iter() {
            template.template.push_str(&template.resource(resource));
            template.template.push('\n');
        }

        let secrets_names: Vec<String> = secrets.iter().map(|s| s.unique_name()).collect();

        for function in template.functions.clone() {
            if function.role()? == "endpoint" {
                template
                    .template
                    .push_str(&template.endpoint(&function, &secrets_names)?);
            }

            if function.role()? == "worker" {
                template.template.push_str(
                    &template
                        .worker(&function, &secrets_names)
                        .wrap_err("Failed to build worker template")?,
                );
            }

            template.template.push('\n');
        }

        template.template.push_str(&template.routing());
        Ok(template)
    }

    fn prefixed(&self, names: Vec<&str>) -> String {
        let joined = names.join("D");

        format!(
            "{username}D{crat_name}D{joined}",
            username = &self.username_escaped,
            crat_name = &self.crat.name
        )
    }

    /// Policy statements to allow a function to access a resource
    ///
    /// Current all functions in a crate have access to all resources. Including secrets.
    fn policies(&self, secrets: &[String]) -> String {
        let mut template = String::default();

        for resource in self.crat.resources.iter() {
            template.push_str(&match resource {
                Resource::KvDb(kvdb) => {
                    let name = self.prefixed(vec![&kvdb.name]);

                    format!(
                        "
                - PolicyName: DynamoPolicy{name}
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
                          - DynamoDBTable{name}
                          - Arn",
                    )
                }

                _ => String::new(),
            })
        }

        // https://docs.aws.amazon.com/systems-manager/latest/userguide/sysman-paramstore-access.html#sysman-paramstore-access-inst
        for secret in secrets.iter() {
            let name = self.prefixed(vec![&secret]);

            template.push_str(&format!(
                "
                - PolicyName: SecretPolicy{name}
                  PolicyDocument:
                      Version: '2012-10-17'
                      Statement:
                      - Effect: Allow
                        Action:
                          - ssm:GetParameter
                          - ssm:GetParameters
                          - ssm:ListTagsForResource
                        Resource:
                          - arn:aws:ssm:us-east-1:727082259008:parameter/{secret}
                      - Effect: Allow
                        Action:
                          - kms:Decrypt
                        Resource:
                          - arn:aws:kms:us-east-1:727082259008:key/1bf38d51-e7e3-4c20-b155-60c6214b0255",
            ));
        }

        template
    }

    /// Define environment variables for a function
    fn environment(&self, function: &Function, secrets: &[String]) -> eyre::Result<String> {
        let raw = function.environment()?;
        let mut raw = raw.as_table().unwrap().clone();

        // If user tries to redefine these values, insert()s will overwrite them
        raw.insert(
            "KINETICS_SECRETS_NAMES".into(),
            Value::String(secrets.join(",")),
        );

        raw.insert(
            "KINETICS_USERNAME".into(),
            Value::String(self.username.clone()),
        );

        let variables = raw
            .iter()
            .map(|(k, v)| format!("                            {k}: {v}"))
            .collect::<Vec<String>>()
            .join("\n");

        if variables.is_empty() {
            return Ok("".to_string());
        }

        Ok(format!(
            "Environment:
                        Variables:
{}",
            variables,
        ))
    }

    /// CFN template for a REST endpoint function — a function itself and its role
    ///
    /// The "secrets" argument is a list of AWS secrets names.
    fn endpoint(&self, function: &Function, secrets: &[String]) -> eyre::Result<String> {
        let policies = self.policies(secrets);
        let name = self.prefixed(vec![&function.name()?]);
        let environment = self.environment(function, secrets)?;
        let bucket = self.bucket.clone();
        let username = self.username.clone();

        // By default a lambda has no permissions to modify its own tags,
        // so it's safe to assign tags with system information and rely on them
        // in other parts of the stack.
        Ok(format!(
            "
            Endpoint{name}:
              Type: AWS::Lambda::Function
              Properties:
                FunctionName: {name}
                Handler: bootstrap
                Runtime: provided.al2023
                {environment}
                Role:
                    Fn::GetAtt:
                    - EndpointRole{name}
                    - Arn
                MemorySize: 256
                Timeout: 1
                ReservedConcurrentExecutions: 8
                Tags:
                    - Key: KINETICS_USERNAME
                      Value: {username}
                Code:
                    S3Bucket: {bucket}
                    S3Key: {s3key}
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
            s3key = function.s3key,
        ))
    }

    /// CFN template for a worker function
    fn worker(&self, function: &Function, secrets: &[String]) -> eyre::Result<String> {
        let policies = self.policies(secrets);
        let name = self.prefixed(vec![&function.name()?]);
        let environment = self.environment(function, secrets)?;
        let bucket = self.bucket.clone();
        let username = self.username.clone();

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
                    {environment}
                    Role:
                      Fn::GetAtt:
                        - WorkerRole{name}
                        - Arn
                    MemorySize: 128
                    Timeout: 3
                    ReservedConcurrentExecutions: 8
                    Code:
                      S3Bucket: {bucket}
                      S3Key: {s3key}
                    Tags:
                        - Key: KINETICS_USERNAME
                        Value: {username}

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
                    QueueName: {queue_name}
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
                        MaximumConcurrency: {queue_concurrency}
            ",
            s3key = function.s3key,
            queue_name = self.prefixed(vec![&queue.name]),
            queue_concurrency = queue.concurrency,
        ))
    }

    /// Check if the stack already exists
    async fn is_exists(&self, name: &str) -> eyre::Result<bool> {
        let result = self
            .client
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

    /// Provision the template in CloudFormation
    pub async fn provision(&self) -> eyre::Result<()> {
        let base_name = self.crat.name.as_str();
        let name = format!("{}-{}", self.username_escaped, base_name);
        let capabilities = aws_sdk_cloudformation::types::Capability::CapabilityIam;

        if self.is_exists(&name).await? {
            self.client
                .update_stack()
                .capabilities(capabilities)
                .stack_name(name)
                .template_body(self.template.clone())
                .send()
                .await
                .wrap_err("Failed to update stack")?;
        } else {
            self.client
                .create_stack()
                .capabilities(capabilities)
                .stack_name(name)
                .template_body(self.template.clone())
                .send()
                .await
                .wrap_err("Failed to create stack")?;
        }

        Ok(())
    }
}

impl std::fmt::Display for Template {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.template.clone())
    }
}
