use crate::api::client::Client;
use crate::api::orgs::create::{Request, Response};
use eyre::{eyre, Context};
use serde::Serialize;

#[derive(Serialize)]
pub(crate) struct Org {
    id: String,
    name: String,
}

pub(crate) struct OrgBuilder;

impl OrgBuilder {
    pub async fn create(name: &str) -> eyre::Result<Org> {
        let response = Client::new(false)
            .await
            .inspect_err(|e| log::error!("Error: {e:?}"))
            .wrap_err("Failed to create client")?
            .post("/orgs/create")
            .json(&Request {
                name: name.to_owned(),
            })
            .send()
            .await
            .wrap_err("Request to API failed")?;

        if response.status() != 200 {
            return Err(eyre!(
                "{}",
                response
                    .json::<serde_json::Value>()
                    .await
                    .inspect_err(|e| log::error!("Error: {e:?}"))
                    .wrap_err("Failed to parse response JSON")?
                    .get("error")
                    .unwrap_or(&serde_json::Value::String(String::from("Unknown error")))
                    .as_str()
                    .unwrap_or_default()
            ));
        }

        let response: Response = response
            .json()
            .await
            .inspect_err(|e| log::error!("Error: {e}"))
            .wrap_err("Failed to parse response JSON")?;

        Ok(Org {
            id: response.id,
            name: name.to_string(),
        })
    }
}
