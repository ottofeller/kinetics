use crate::function::Function;
use crate::secret::Secret;
use crate::{Crate, Resource};
use eyre::{ContextCompat, Ok, WrapErr};
use toml::Value;

#[derive(Clone)]
pub struct Template {
    crat: Crate,
    functions: Vec<Function>,
    pub template: String,
}

impl Template {
    /// CFN template for a resource (e.g. a queue or a db)
    fn resource(&self, resource: &Resource) -> String {
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

    /// Domain and paths for endpoint lambdas
    fn routing(&self) -> String {
        let project_name = self.crat.name.clone();
        let functions: Vec<Function> = self.functions.clone();

        let functions = functions
            .iter()
            .filter(|f| f.role().unwrap() == "endpoint")
            .collect::<Vec<&Function>>();

        let default_origin_name = functions[0].name().unwrap();

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
                    name = f.name().unwrap()
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
                    name = f.name().unwrap()
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

    pub fn new(crat: &Crate, functions: Vec<Function>, secrets: Vec<Secret>) -> eyre::Result<Self> {
        let mut template = Template {
            crat: crat.clone(),
            template: "Resources:".to_string(),
            functions,
        };

        // Define global resources from the app's Cargo.toml, e.g. a DB
        for resource in crat.resources.iter() {
            template.template.push_str(&template.resource(&resource));
            template.template.push_str("\n");
        }

        let secrets_names = secrets.iter().map(|s| s.unique_name()).collect();

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

            template.template.push_str("\n");
        }

        template.template.push_str(&template.routing());
        Ok(template)
    }

    /// Policy statements to allow a function to access a resource
    ///
    /// Current all functions in a crate have access to all resources. Including secrets.
    fn policies(&self, secrets: &Vec<String>) -> String {
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

        // https://docs.aws.amazon.com/systems-manager/latest/userguide/sysman-paramstore-access.html#sysman-paramstore-access-inst
        for secret in secrets.iter() {
            template.push_str(&format!(
                "
                - PolicyName: SecretPolicy{}
                  PolicyDocument:
                      Version: '2012-10-17'
                      Statement:
                      - Effect: Allow
                        Action:
                          - ssm:GetParameter
                        Resource:
                          - arn:aws:ssm:us-east-1:727082259008:parameter/{}
                      - Effect: Allow
                        Action:
                          - kms:Decrypt
                        Resource:
                          - arn:aws:kms:us-east-1:727082259008:key/1bf38d51-e7e3-4c20-b155-60c6214b0255",
                secret, secret,
            ));
        }

        template
    }

    /// Define environment variables for a function
    fn environment(&self, function: &Function, secrets: &Vec<String>) -> eyre::Result<String> {
        let raw = function.environment()?;
        let mut raw = raw.as_table().unwrap().clone();
        raw.insert("SECRETS_NAMES".into(), Value::String(secrets.join(",")));

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
    fn endpoint(&self, function: &Function, secrets: &Vec<String>) -> eyre::Result<String> {
        let policies = self.policies(secrets);
        let name = function.name()?;
        let environment = self.environment(function, secrets)?;

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
    fn worker(&self, function: &Function, secrets: &Vec<String>) -> eyre::Result<String> {
        let policies = self.policies(secrets);
        let name = function.name()?;
        let environment = self.environment(function, secrets)?;

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
