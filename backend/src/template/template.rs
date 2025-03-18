use crate::config::config as build_config;
use crate::template::Crate;
use crate::template::Function;
use crate::template::Secret;
use crate::Resource;
use aws_config::BehaviorVersion;
use eyre::{ContextCompat, Ok, WrapErr};
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct Template {
    /// AWS account ID
    account_id: String,

    bucket: String,
    client: aws_sdk_cloudformation::Client,
    crat: Crate,
    functions: Vec<Function>,
    username_escaped: String,
    username: String,
    domain_name: Option<String>,
    template: Value,
}

#[derive(Clone, Debug)]
pub struct CfnResource {
    name: String,
    resource: Value,
}

impl Template {
    /// Add a resource to the CFN template
    fn add_resource(&mut self, CfnResource { name, resource }: CfnResource) {
        self.template
            .get_mut("Resources")
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(name, resource);
    }

    /// CFN template for a resource (e.g. a queue or a db)
    fn resource(&self, resource: &Resource) -> CfnResource {
        match resource {
            Resource::KvDb(kvdb) => CfnResource {
                name: format!(
                    "DynamoDBTable{name}",
                    name = self.prefixed(vec![&kvdb.name])
                ),
                resource: json!({
                        "Type": "AWS::DynamoDB::Table",
                        "Properties": {
                            "TableName": kvdb.name,
                            "AttributeDefinitions": [{
                                "AttributeName": "id",
                                "AttributeType": "S"
                            }],
                            "KeySchema": [{
                                "AttributeName": "id",
                                "KeyType": "HASH"
                            }],
                            "ProvisionedThroughput": {
                                "ReadCapacityUnits": 5,
                                "WriteCapacityUnits": 5
                            }
                        }
                }),
            },

            _ => unimplemented!(),
        }
    }

    /// Domain and paths for endpoint lambdas
    fn routing(&self) -> Vec<CfnResource> {
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
                let name = self.prefixed(vec![&f.name().unwrap()]);
                json!({
                    "Id": format!("EndpointOrigin{name}"),
                    "DomainName": {
                        "Fn::Select": [2, {"Fn::Split": ['/', { "Fn::GetAtt" : [format!("EndpointUrl{name}"), "FunctionUrl"]}]}]
                    },
                    "CustomOriginConfig": {
                        "OriginProtocolPolicy": "https-only"
                    }
                })
            })
            .collect::<Vec<Value>>();

        let behaviors = functions
            .iter()
            .flat_map(|f| {
                // Manage trailing slashes in the template
                let re = regex::Regex::new(r"/$").unwrap();
                let url_path = &f.url_path().unwrap_or(f.name().unwrap().to_lowercase());
                let path = re.replace_all(url_path, "");
                let name = self.prefixed(vec![&f.name().unwrap()]);

                [path.to_string(), format!("{path}/")].map(|p| {
                    json!({
                        "PathPattern": p,
                        "AllowedMethods": [
                            "DELETE",
                            "GET",
                            "HEAD",
                            "OPTIONS",
                            "PATCH",
                            "POST",
                            "PUT",
                        ],
                        "OriginRequestPolicyId": "b689b0a8-53d0-40ab-baf2-68738e2966ac",
                        "CachePolicyId": "4135ea2d-6df8-44a3-9df3-4b5a84be39ad",
                        "ForwardedValues": {"QueryString": true},
                        "TargetOriginId": format!("EndpointOrigin{name}"),
                        "ViewerProtocolPolicy": "redirect-to-https",
                        "Compress": true
                    })
                })
            })
            .collect::<Vec<Value>>();

        let project_domain = self
            .domain_name
            .as_ref()
            .map(|domain_name| format!("{project_name}.{domain_name}"));

        let mut resources = vec![CfnResource {
            name: format!("EndpointDistribution{project_name}"),
            resource: json!({
                "Type": "AWS::CloudFront::Distribution",
                "Properties": {
                    "DistributionConfig": {
                        "Aliases": if let Some(ref project_domain) = project_domain {
                            vec![project_domain]
                        } else {
                            vec![]
                        },
                        "Enabled": true,
                        "CacheBehaviors": behaviors,
                        "DefaultCacheBehavior": {
                            "AllowedMethods": [
                                "DELETE",
                                "GET",
                                "HEAD",
                                "OPTIONS",
                                "PATCH",
                                "POST",
                                "PUT",
                            ],
                            "DefaultTTL": 0,
                            "MaxTTL": 0,
                            "MinTTL": 0,
                            "ForwardedValues": {
                                "QueryString": true,
                                "Headers": ["*"],
                                "Cookies": {"Forward": "all"}
                            },
                            "TargetOriginId": format!("EndpointOrigin{default_origin_name}"),
                            "ViewerProtocolPolicy": "allow-all",
                            "Compress": true
                        },
                        "Origins": origins,
                        "ViewerCertificate": if project_domain.is_some() {
                            json!({
                                "AcmCertificateArn": {"Ref": format!("EndpointDistributionDomainCert{project_name}")},
                                "SslSupportMethod": "sni-only",
                                "MinimumProtocolVersion": "TLSv1"
                            })
                        } else {
                            json!({
                                "CloudFrontDefaultCertificate": true
                            })
                        }
                    }
                }
            }),
        }];

        // Add Certificate Manager resources for custom defined domain
        if let Some(project_domain) = project_domain {
            let hosted_zone_id = build_config().hosted_zone_id;

            resources.extend([
                CfnResource {
                    name: format!("EndpointDistributionDomainCert{project_name}"),
                    resource: json!({
                        "Type": "AWS::CertificateManager::Certificate",
                        "Properties": {
                            "DomainName": project_domain,
                            "ValidationMethod": "DNS",
                            "DomainValidationOptions": [{
                                "DomainName": project_domain,
                                "HostedZoneId": hosted_zone_id,
                            }]
                        }
                    }),
                },
                CfnResource {
                    name: format!("EndpointDistributionAliasRecord{project_name}"),
                    resource: json!({
                        "Type": "AWS::Route53::RecordSet",
                        "Properties": {
                            "HostedZoneId": hosted_zone_id,
                            "Name": project_domain,
                            "Type": "A",
                            "AliasTarget": {
                                "HostedZoneId": "Z2FDTNDATAQYW2", // CloudFront Hosted Zone ID
                                "DNSName": {
                                    "Fn::GetAtt": [
                                        format!("EndpointDistribution{project_name}"),
                                        "DomainName"
                                    ]
                                }
                            }
                        }
                    }),
                },
            ]);
        }

        resources
    }

    pub async fn new(
        crat: &Crate,
        functions: Vec<Function>,
        secrets: Vec<Secret>,
        bucket: &str,
        username_escaped: &str,
        username: &str,
        domain_name: Option<&str>,
    ) -> eyre::Result<Self> {
        let config = aws_config::defaults(BehaviorVersion::v2025_01_17())
            .load()
            .await;

        let client = aws_sdk_cloudformation::Client::new(&config);

        let sts_client = aws_sdk_sts::Client::new(&config);
        let identify = sts_client.get_caller_identity().send().await?;

        let account_id = identify
            .account()
            .ok_or_else(|| eyre::Error::msg("Failed to get AWS account ID"))?;

        let mut template = Template {
            account_id: account_id.to_string(),
            bucket: bucket.to_string(),
            client,
            crat: crat.clone(),
            template: json!({"Resources": {}}),
            username_escaped: username_escaped.to_string(),
            username: username.to_string(),
            functions,
            domain_name: domain_name.map(|d| d.to_string()),
        };

        // Define global resources from the app's Cargo.toml, e.g. a DB
        for resource in crat.resources.iter() {
            template.add_resource(template.resource(resource));
        }

        let secrets_names: Vec<String> = secrets.iter().map(|s| s.unique_name()).collect();

        for function in template.functions.clone() {
            if function.role()? == "endpoint" {
                for resource in template.endpoint(&function, &secrets_names)? {
                    template.add_resource(resource);
                }
            }

            if function.role()? == "cron" {
                for resource in template
                    .cron(&function, &secrets_names)
                    .wrap_err("Failed to build cron template")?
                {
                    template.add_resource(resource);
                }
            }

            if function.role()? == "worker" {
                for resource in template
                    .worker(&function, &secrets_names)
                    .wrap_err("Failed to build worker template")?
                {
                    template.add_resource(resource);
                }
            }
        }

        for resource in template.routing() {
            template.add_resource(resource);
        }

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
    fn policies(&self, secrets: &[String]) -> Vec<Value> {
        let mut template = Vec::new();

        for resource in self.crat.resources.iter() {
            if let Resource::KvDb(kvdb) = resource {
                let name = self.prefixed(vec![&kvdb.name]);

                template.push(json!({
                    "PolicyName": format!("DynamoPolicy{name}"),
                    "PolicyDocument": {
                        "Version": "2012-10-17",
                        "Statement": [{
                            "Effect": "Allow",
                            "Action": [
                                "dynamodb:BatchGetItem",
                                "dynamodb:BatchWriteItem",
                                "dynamodb:ConditionCheckItem",
                                "dynamodb:PutItem",
                                "dynamodb:DescribeTable",
                                "dynamodb:DeleteItem",
                                "dynamodb:GetItem",
                                "dynamodb:Scan",
                                "dynamodb:Query",
                                "dynamodb:UpdateItem"
                            ],
                            "Resource": {
                                "Fn::GetAtt": [
                                    format!("DynamoDBTable{name}"),
                                    "Arn"
                                ]
                            }
                        }]
                    }
                }))
            }
        }

        let account_id = self.account_id.clone();
        let kms_key_id = build_config().kms_key_id;

        // https://docs.aws.amazon.com/systems-manager/latest/userguide/sysman-paramstore-access.html#sysman-paramstore-access-inst
        for secret in secrets.iter() {
            let name = self.prefixed(vec![&secret]);

            template.push(json!({
                "PolicyName": format!("SecretPolicy{name}"),
                "PolicyDocument": {
                    "Version": "2012-10-17",
                    "Statement": [
                        {
                            "Effect": "Allow",
                            "Action": [
                                "ssm:GetParameter",
                                "ssm:GetParameters",
                                "ssm:ListTagsForResource"
                            ],
                            "Resource": [format!("arn:aws:ssm:us-east-1:{account_id}:parameter/{secret}")]
                        },
                        {
                            "Effect": "Allow",
                            "Action": ["kms:Decrypt"],
                            "Resource": [format!("arn:aws:kms:us-east-1:{account_id}:key/{kms_key_id}")]
                        }
                    ]
                }
            }));
        }

        template
    }

    /// Define environment variables for a function
    fn environment(&self, function: &Function, secrets: &[String]) -> eyre::Result<Value> {
        let raw = function.environment()?;
        let raw = raw.as_table().unwrap().clone();

        let mut variables = raw
            .iter()
            .map(|(name, resource)| {
                (
                    name.clone(),
                    serde_json::to_value(resource)
                        .wrap_err("Failed to serialize environment variable")
                        .unwrap(),
                )
            })
            .collect::<serde_json::Map<String, Value>>();

        // If user tries to redefine these values, insert()s will overwrite them
        variables.insert(
            "KINETICS_SECRETS_NAMES".into(),
            Value::String(secrets.join(",")),
        );

        variables.insert(
            "KINETICS_USERNAME".into(),
            Value::String(self.username.clone()),
        );

        Ok(json!({"Variables": variables}))
    }

    /// CFN template for a REST endpoint function — a function itself and its role
    ///
    /// The "secrets" argument is a list of AWS secrets names.
    fn endpoint(&self, function: &Function, secrets: &[String]) -> eyre::Result<Vec<CfnResource>> {
        let mut policies = self.policies(secrets);
        policies.push(json!({
            "PolicyName": "AppendToLogsPolicy",
            "PolicyDocument": {
                "Version": "2012-10-17",
                "Statement": [{
                    "Effect": "Allow",
                    "Action": [
                        "logs:CreateLogGroup",
                        "logs:CreateLogStream",
                        "logs:PutLogEvents"
                    ],
                    "Resource": "*"
                }]
            }
        }));
        let name = self.prefixed(vec![&function.name()?]);
        let environment = self.environment(function, secrets)?;
        let bucket = self.bucket.clone();
        let username = self.username.clone();
        let Function { s3key, .. } = function;

        // By default a lambda has no permissions to modify its own tags,
        // so it's safe to assign tags with system information and rely on them
        // in other parts of the stack.
        Ok(vec![
            CfnResource {
                name: format!("Endpoint{name}"),
                resource: json!({
                        "Type": "AWS::Lambda::Function",
                        "Properties": {
                            "FunctionName": name,
                            "Handler": "bootstrap",
                            "Runtime": "provided.al2023",
                            "Environment": environment,
                            "Role": {
                                "Fn::GetAtt": [
                                    format!("EndpointRole{name}"),
                                    "Arn"
                                ]
                            },
                            "MemorySize": 256,
                            "Timeout": 1,
                            "Code": {
                                "S3Bucket": bucket,
                                "S3Key": s3key
                            },
                           "Tags": [{
                               "Key": "KINETICS_USERNAME",
                               "Value": username
                           }]
                        }
                }),
            },
            CfnResource {
                name: format!("EndpointRole{name}"),
                resource: json!({
                    "Type": "AWS::IAM::Role",
                    "Properties": {
                        "AssumeRolePolicyDocument": {
                            "Version": "2012-10-17",
                            "Statement": [{
                                "Effect": "Allow",
                                "Principal": {
                                    "Service": ["lambda.amazonaws.com"]
                                },
                                "Action": ["sts:AssumeRole"]
                            }]
                        },
                        "Path": "/",
                        "Policies": policies
                    }
                }),
            },
            CfnResource {
                name: format!("EndpointUrl{name}"),
                resource: json!({
                    "Type": "AWS::Lambda::Url",
                    "Properties": {
                        "AuthType": "NONE",
                        "TargetFunctionArn": {"Ref" : format!("Endpoint{name}")}
                    }
                }),
            },
            CfnResource {
                name: format!("EndpointUrlPermission{name}"),
                resource: json!({
                    "Type": "AWS::Lambda::Permission",
                    "Properties": {
                        "Action": "lambda:InvokeFunctionUrl",
                        "FunctionUrlAuthType": "NONE",
                        "FunctionName": {"Ref" : format!("Endpoint{name}")},
                        "Principal": "*"
                    }
                }),
            },
        ])
    }

    /// CFN template for a worker function
    fn worker(&self, function: &Function, secrets: &[String]) -> eyre::Result<Vec<CfnResource>> {
        let name = self.prefixed(vec![&function.name()?]);
        let environment = self.environment(function, secrets)?;
        let bucket = self.bucket.clone();
        let username = self.username.clone();

        let mut policies = self.policies(secrets);
        policies.extend([
            json!({
                "PolicyName": "AppendToLogsPolicy",
                "PolicyDocument": {
                    "Version": "2012-10-17",
                    "Statement": [{
                        "Effect": "Allow",
                        "Action": [
                            "logs:CreateLogGroup",
                            "logs:CreateLogStream",
                            "logs:PutLogEvents"
                        ],
                        "Resource": "*"
                    }]
                },
            }),
            json!({
                "PolicyName": "QueuePolicy",
                "PolicyDocument": {
                    "Version": "2012-10-17",
                    "Statement": [{
                        "Effect": "Allow",
                        "Action": [
                            "sqs:ChangeMessageVisibility",
                            "sqs:DeleteMessage",
                            "sqs:GetQueueAttributes",
                            "sqs:GetQueueUrl",
                            "sqs:ReceiveMessage"
                        ],
                        "Resource": {
                            "Fn::GetAtt": [
                                format!("WorkerQueue{name}"),
                                "Arn"
                            ]
                        }
                    }]
                }
            }),
        ]);

        let queue = function
            .resources()
            .iter()
            .find_map(|r| match r {
                Resource::Queue(queue) => Some(queue),
                _ => None,
            })
            .wrap_err("No queue resource found in Cargo.toml")?;

        Ok(vec![
            CfnResource {
                name: format!("Worker{name}"),
                resource: json!({
                    "Type": "AWS::Lambda::Function",
                    "Properties": {
                        "FunctionName": name,
                        "Handler": "bootstrap",
                        "Runtime": "provided.al2023",
                        "Environment": environment,
                        "Role": {
                            "Fn::GetAtt": [
                                format!("WorkerRole{name}"),
                                "Arn"
                            ]
                        },
                        "MemorySize": 128,
                        "Timeout": 3,
                        "Code": {
                            "S3Bucket": bucket,
                            "S3Key": function.s3key
                        },
                        "Tags": [{
                            "Key": "KINETICS_USERNAME",
                            "Value": username
                        }]
                    }
                }),
            },
            CfnResource {
                name: format!("WorkerRole{name}"),
                resource: json!({
                    "Type": "AWS::IAM::Role",
                    "Properties": {
                        "AssumeRolePolicyDocument": {
                            "Version": "2012-10-17",
                            "Statement": [{
                                "Effect": "Allow",
                                "Principal": {
                                    "Service": ["lambda.amazonaws.com"]
                                },
                                "Action": ["sts:AssumeRole"]
                            }]
                        },
                        "Path": "/",
                        "Policies": policies
                    }
                }),
            },
            CfnResource {
                name: format!("WorkerQueue{name}"),
                resource: json!({
                    "Type": "AWS::SQS::Queue",
                    "Properties": {
                        "QueueName": self.prefixed(vec![&queue.name]),
                        "VisibilityTimeout": 60,
                        "MaximumMessageSize": 2048,
                        "MessageRetentionPeriod": 345600,
                        "ReceiveMessageWaitTimeSeconds": 20
                    }
                }),
            },
            CfnResource {
                name: format!("WorkerQueueEventSourceMapping{name}"),
                resource: json!({
                    "Type": "AWS::Lambda::EventSourceMapping",
                    "Properties": {
                        "EventSourceArn": {
                            "Fn::GetAtt": [
                                format!("WorkerQueue{name}"),
                                "Arn"
                            ]
                        },
                        "FunctionName": {"Ref": format!("Worker{name}")},
                        "ScalingConfig": {"MaximumConcurrency": queue.concurrency}
                    }
                }),
            },
        ])
    }

    /// CFN template for a worker function
    fn cron(&self, function: &Function, secrets: &[String]) -> eyre::Result<Vec<CfnResource>> {
        let name = self.prefixed(vec![&function.name()?]);
        let environment = self.environment(function, secrets)?;
        let bucket = self.bucket.clone();
        let username = self.username.clone();
        let mut policies = self.policies(secrets);

        policies.extend([json!({
            "PolicyName": "AppendToLogsPolicy",
            "PolicyDocument": {
                "Version": "2012-10-17",
                "Statement": [{
                    "Effect": "Allow",
                    "Action": [
                        "logs:CreateLogGroup",
                        "logs:CreateLogStream",
                        "logs:PutLogEvents"
                    ],
                    "Resource": "*"
                }]
            },
        })]);

        Ok(vec![
            CfnResource {
                name: format!("Cron{name}"),

                resource: json!({
                    "Type": "AWS::Lambda::Function",
                    "Properties": {
                        "FunctionName": name,
                        "Handler": "bootstrap",
                        "Runtime": "provided.al2023",
                        "Environment": environment,
                        "Role": {"Fn::GetAtt": [format!("CronRole{name}"), "Arn"]},
                        "MemorySize": 128,
                        "Timeout": 3,
                        "ReservedConcurrentExecutions": 8,
                        "Code": {"S3Bucket": bucket, "S3Key": function.s3key},
                        "Tags": [{"Key": "KINETICS_USERNAME", "Value": username}]
                    }
                }),
            },
            CfnResource {
                name: format!("CronRole{name}"),

                resource: json!({
                    "Type": "AWS::IAM::Role",
                    "Properties": {
                        "AssumeRolePolicyDocument": {
                            "Version": "2012-10-17",
                            "Statement": [{
                                "Effect": "Allow",
                                "Principal": {"Service": ["lambda.amazonaws.com"]},
                                "Action": ["sts:AssumeRole"]
                            }]
                        },
                        "Path": "/",
                        "Policies": policies
                    }
                }),
            },
            CfnResource {
                name: format!("CronEventBridgeRule{name}"),

                resource: json!({
                    "Type": "AWS::Events::Rule",
                    "Properties": {
                        "Description": "EventBridge rule to trigger cron lambda",
                        "ScheduleExpression": function.schedule().unwrap(),
                        "State": "ENABLED",
                        "Targets": [{
                            "Arn": {"Fn::GetAtt": [format!("Cron{name}"), "Arn"]},
                            "Id": format!("CronTarget{name}")
                        }]
                    }
                }),
            },
            CfnResource {
                name: format!("CronEventBridgePermission{name}"),

                resource: json!({
                    "Type": "AWS::Lambda::Permission",
                    "Properties": {
                        "Action": "lambda:InvokeFunction",
                        "FunctionName": {"Ref": format!("Cron{name}")},
                        "Principal": "events.amazonaws.com",
                        "SourceArn": {"Fn::GetAtt": [format!("CronEventBridgeRule{name}"), "Arn"]}
                    }
                }),
            },
        ])
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
        let template_string = serde_json::to_string_pretty(&self.template)?;

        if self.is_exists(&name).await? {
            self.client
                .update_stack()
                .capabilities(capabilities)
                .stack_name(name)
                .template_body(template_string)
                .send()
                .await
                .wrap_err("Failed to update stack")?;
        } else {
            self.client
                .create_stack()
                .capabilities(capabilities)
                .stack_name(name)
                .template_body(template_string)
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
