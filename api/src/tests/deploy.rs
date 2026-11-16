use crate::{request::Validate, stack::deploy::Request};
use serde_json::{Value, json};

fn deploy_request() -> Value {
    json!({
        "is_hotswap": false,
        "project": {"name": "shop", "url": "", "kvdb": []},
        "secrets": {"GLOBAL_TOKEN": "project-value"},
        "functions": [{
            "is_deploying": true,
            "package": "payments",
            "name": "Run",
            "role": "Cron",
            "params": {"Cron": {"schedule": "rate(1 minute)", "environment": {}}},
            "environment": {},
            "secrets": {"values": {"TOKEN": "package-value"}}
        }]
    })
}

#[test]
fn rejects_deploy_without_package_identity() {
    let mut json = deploy_request();
    json["functions"][0]
        .as_object_mut()
        .unwrap()
        .remove("package");
    assert!(serde_json::from_value::<Request>(json).is_err());

    let mut json = deploy_request();
    json["functions"][0]["package"] = json!("");
    let request: Request = serde_json::from_value(json).unwrap();
    assert_eq!(
        request.validate().unwrap(),
        ["Invalid function package. Must not be empty."]
    );
}

#[test]
fn accepts_function_secrets_without_a_separate_scope() {
    for is_hotswap in [false, true] {
        let mut json = deploy_request();
        json["is_hotswap"] = json!(is_hotswap);
        let request: Request = serde_json::from_value(json).unwrap();
        assert!(request.validate().is_none());
        assert_eq!(request.functions[0].package, "payments");
        assert_eq!(
            serde_json::to_value(&request.functions[0].secrets).unwrap(),
            json!({"values": {"TOKEN": "package-value"}})
        );
    }
}
