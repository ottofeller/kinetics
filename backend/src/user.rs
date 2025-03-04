use aws_sdk_dynamodb::types::AttributeValue::S;
use eyre::Ok;
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

            return Ok(User { email, id: user_id });
        }

        return Ok(User {
            email,
            id: item.unwrap()["id"].as_s().unwrap().to_string(),
        });
    }
}

pub struct User {
    pub email: String,
    pub id: String,
}
