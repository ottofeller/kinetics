use crate::tools::tests::runtime_json;

#[tokio::test]
async fn kinetics_config_uses_the_supplied_snapshot() {
    use crate::tools::config::{Config, EndpointConfig};
    use aws_config::{Region, SdkConfig};
    use aws_sdk_dynamodb::config::{Credentials, SharedCredentialsProvider};

    let mut sdk = SdkConfig::builder()
        .region(Region::new("eu-west-1"))
        .credentials_provider(SharedCredentialsProvider::new(Credentials::new(
            "test-key",
            "test-secret",
            None,
            None,
            "test",
        )));
    sdk.set_time_source(Some(Default::default()));
    let sdk = sdk.build();
    let config = Config::from_runtime(
        &sdk,
        &serde_json::from_value(runtime_json()).unwrap(),
        Some(EndpointConfig::new("/test/{id}")),
    )
    .await
    .unwrap();

    assert_eq!(config.db.endpoint(), "test-cluster.dsql.eu-west-1.on.aws");
    assert_eq!(config.db.username(), "test-user");
    assert_eq!(config.url_pattern().map(String::as_str), Some("/test/{id}"));
}

#[test]
fn local_runtime_initializes_without_cloud_configuration() {
    // A separate process avoids changing the environment of concurrent tests.
    const CHILD: &str = "KINETICS_TEST_LOCAL_RUNTIME";
    if let Ok(with_database) = std::env::var(CHILD) {
        use crate::tools::{
            config::EndpointConfig,
            runtime::{Environment, Runtime},
        };
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let runtime =
                    Runtime::load(Environment::Local, Some(EndpointConfig::new("/test/{id}")))
                        .await
                        .unwrap();
                assert_eq!(runtime.secrets.len(), 2);
                assert_eq!(runtime.secrets["TOKEN"], "local-value");
                assert_eq!(runtime.secrets["KINETICS_SECRET_TOKEN"], "nested-prefix");
                let (endpoint, username) = if with_database == "true" {
                    ("local-db", "local-user")
                } else {
                    ("localhost", "user")
                };
                assert_eq!(runtime.config.db.endpoint(), endpoint);
                assert_eq!(runtime.config.db.username(), username);
                assert_eq!(
                    runtime.config.url_pattern().map(String::as_str),
                    Some("/test/{id}")
                );
            });
        return;
    }

    for with_database in [false, true] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                concat!(
                    module_path!(),
                    "::local_runtime_initializes_without_cloud_configuration"
                ),
                "--nocapture",
            ])
            .env_clear()
            .env(CHILD, with_database.to_string())
            .env("AWS_EC2_METADATA_DISABLED", "true")
            .env("AWS_CONFIG_FILE", "/dev/null")
            .env("AWS_SHARED_CREDENTIALS_FILE", "/dev/null")
            .env("AWS_REGION", "us-east-1")
            .env("KINETICS_SECRET_TOKEN", "local-value")
            .env("KINETICS_SECRET_KINETICS_SECRET_TOKEN", "nested-prefix")
            .env("KINETICS_SQLDB_CLUSTER_ID", "ignored-cloud-cluster")
            .env("KINETICS_SQLDB_USER", "ignored-cloud-user");
        if with_database {
            command.env(
                "KINETICS_SQLDB_LOCAL_CONNECTION_STRING",
                "postgres://local-user:local-password@local-db:5432/postgres",
            );
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
