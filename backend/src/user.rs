use aws_config::BehaviorVersion;
use aws_sdk_cloudwatch::types::{Dimension, Statistic};
use aws_sdk_dynamodb::types::AttributeValue::{self, S};
use aws_sdk_s3::primitives::{DateTime, DateTimeFormat::DateTimeWithOffset};
use chrono::{DateTime as ChronoDateTime, Datelike, Timelike, Utc};
use eyre::{Context, ContextCompat, Ok};
use std::collections::HashMap;

pub struct UserBuilder {
    db_client: aws_sdk_dynamodb::Client,
    db_table: String,
}

impl UserBuilder {
    pub fn new(db_client: &aws_sdk_dynamodb::Client, db_table: &str) -> Self {
        UserBuilder {
            db_client: db_client.clone(),
            db_table: db_table.to_string(),
        }
    }

    /// Create user if it does not exist
    ///
    /// Return existing user if it exists
    pub async fn create(&self, email: String) -> eyre::Result<User> {
        let now = chrono::Utc::now();

        let item = self
            .db_client
            .get_item()
            .table_name(&self.db_table)
            .key("id", S(format!("email#{}", email)))
            .send()
            .await?
            .item;

        // Create user record if it doesn't exist
        if item.is_none() {
            let user_id = uuid::Uuid::new_v4().to_string();

            self.db_client
                .put_item()
                .table_name(&self.db_table)
                .set_item(Some(HashMap::from([
                    ("id".to_string(), S(format!("user#{}", user_id))),
                    ("email".to_string(), S(email.clone())),
                    ("created_at".to_string(), S(now.to_rfc3339())),
                ])))
                .send()
                .await?;

            // Store email separately
            self.db_client
                .put_item()
                .table_name(&self.db_table)
                .set_item(Some(HashMap::from([
                    ("id".to_string(), S(format!("email#{email}"))),
                    ("userId".to_string(), S(user_id.clone())),
                    ("created_at".to_string(), S(now.to_rfc3339())),
                ])))
                .send()
                .await?;

            return Ok(User {
                email: email.to_string().clone(),
                id: user_id,
                throttled_at: None,
                db_client: self.db_client.clone(),
                db_table: self.db_table.clone(),
            });
        }

        return self.to_user(item.unwrap());
    }

    /// Find existing user by email
    ///
    /// Return an error if no user exists with such email
    pub async fn by_email(&self, email: &str) -> eyre::Result<User> {
        let item = self
            .db_client
            .get_item()
            .table_name(&self.db_table)
            .key("id", S(format!("email#{}", email)))
            .send()
            .await?
            .item
            .wrap_err(format!("User with email not found: {email}"));

        let item = item.wrap_err(format!("User record is corrupted: {email}"))?;
        let id = format!("user#{}", item["userId"].as_s().unwrap());

        let item = self
            .db_client
            .get_item()
            .table_name(&self.db_table)
            .key("id", S(id.clone()))
            .send()
            .await?
            .item
            .wrap_err(format!("User with id not found: {id}"))?;

        self.to_user(item)
    }

    /// Build User struct out of item retrieved from DB
    fn to_user(&self, item: HashMap<String, AttributeValue>) -> eyre::Result<User> {
        let throttled_at = item
            .get("throttled_at")
            .unwrap_or(&AttributeValue::S(String::default()))
            .as_s()
            .unwrap_or(&String::default())
            .to_owned();

        Ok(User {
            db_client: self.db_client.clone(),
            db_table: self.db_table.clone(),
            email: item["email"].as_s().unwrap().to_owned(),
            id: item["id"].as_s().unwrap().to_owned().replace("user#", ""),

            throttled_at: if throttled_at.is_empty() {
                None
            } else {
                Some(ChronoDateTime::parse_from_rfc3339(&throttled_at)?.to_utc())
            },
        })
    }
}

pub struct User {
    pub email: String,
    pub id: String,
    pub throttled_at: Option<ChronoDateTime<Utc>>,
    pub db_client: aws_sdk_dynamodb::Client,
    pub db_table: String,
}

impl User {
    /// The list of names of lambda functions deployed for the user
    async fn functions(&self) -> eyre::Result<Vec<String>> {
        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let client = aws_sdk_resourcegroupstagging::Client::new(&config);
        let mut list = vec![];
        let mut has_more = true;
        let mut token = String::default();

        while has_more {
            let mut res = client
                .get_resources()
                .tag_filters(
                    aws_sdk_resourcegroupstagging::types::TagFilter::builder()
                        .key("KINETICS_USERNAME")
                        .values(self.email.clone())
                        .build(),
                )
                .resource_type_filters("lambda:function");

            if !token.is_empty() {
                res = res.set_pagination_token(Some(token.clone()));
            }

            let response = res.send().await?;
            has_more = !response
                .clone()
                .pagination_token()
                .unwrap_or_default()
                .is_empty();

            if response.pagination_token().is_some() {
                token = response.clone().pagination_token.unwrap();
            }

            list.extend(
                response
                    .resource_tag_mapping_list()
                    .iter()
                    .filter_map(|res| res.resource_arn().map(|arn| arn.to_string())),
            );
        }

        Ok(list)
    }

    /// Total number of unvocations for user's functioncs
    pub async fn invocations(&self, period: &str) -> eyre::Result<u16> {
        let mut total = 0;
        let now_raw = Utc::now();
        let end_time = DateTime::from_str(&now_raw.to_rfc3339(), DateTimeWithOffset)?;

        let start_time = match period {
            "month" => DateTime::from_str(
                &now_raw
                    .with_day(1)
                    .unwrap()
                    .with_hour(0)
                    .unwrap()
                    .with_minute(0)
                    .unwrap()
                    .with_second(0)
                    .unwrap()
                    .to_rfc3339(),
                DateTimeWithOffset,
            )
            .unwrap(),
            _ => eyre::bail!("Invalid period"),
        };

        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let cloudwatch_client = aws_sdk_cloudwatch::Client::new(&config);
        let functions = self.functions().await?;

        for function in functions {
            let metrics = cloudwatch_client
                .get_metric_statistics()
                .namespace("AWS/Lambda")
                .metric_name("Invocations")
                .dimensions(
                    Dimension::builder()
                        .name("FunctionName")
                        .value(
                            // There is seemingly no other way to get function name
                            function
                                .clone()
                                .split(":")
                                .last()
                                .unwrap_or_default()
                                .to_string(),
                        )
                        .build(),
                )
                .start_time(start_time)
                .end_time(end_time)
                .period(30 * 24 * 60 * 60) // 30 days in seconds
                .statistics(Statistic::Sum)
                .send()
                .await
                .wrap_err("Failed to get CloudWatch metrics")?;

            total += metrics
                .datapoints()
                .iter()
                .map(|dp| dp.sum().unwrap_or(0.0) as u16)
                .sum::<u16>();
        }

        Ok(total)
    }

    /// Throttle all user's functions and store the state in DB
    pub async fn throttle(&mut self, state: bool) -> eyre::Result<()> {
        // Avoid applying same state multiple times
        if (self.throttled_at.is_some() && state) || (self.throttled_at.is_none() && !state) {
            return Ok(());
        }

        let config = aws_config::load_defaults(BehaviorVersion::latest()).await;
        let lambda_client = aws_sdk_lambda::Client::new(&config);
        let functions = self.functions().await?;

        for function in functions {
            lambda_client
                .put_function_concurrency()
                .function_name(function)
                .reserved_concurrent_executions(if state { 0 } else { 8 })
                .send()
                .await?;
        }

        self.throttled_at = if state {
            Some(chrono::Utc::now())
        } else {
            None
        };

        self.db_client
            .update_item()
            .table_name(&self.db_table)
            .key("id", S(format!("user#{}", self.id)))
            .update_expression("SET throttled_at = :t")
            .expression_attribute_values(
                ":t",
                S(self
                    .throttled_at
                    .map_or(String::default(), |t| t.to_rfc3339())),
            )
            .send()
            .await?;

        Ok(())
    }
}
