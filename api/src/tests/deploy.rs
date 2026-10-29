use crate::{request::Validate, stack::deploy::Request};
use serde_json::{json, Value};

fn legacy_request() -> Value {
    json!({
        "is_hotswap": false,
        "project": {"name": "shop", "url": "", "kvdb": []},
        "secrets": {"GLOBAL_TOKEN": "project-value"},
        "functions": [{
            "is_deploying": true,
            "name": "Run",
            "role": "Cron",
            "params": {"Cron": {"schedule": "rate(1 minute)", "environment": {}}},
            "environment": {},
            "secrets": {"scope": "payments", "values": {"TOKEN": "package-value"}}
        }]
    })
}

#[test]
fn accepts_legacy_deploy_without_package_identity() {
    let request: Request = serde_json::from_value(legacy_request()).unwrap();
    assert!(request.validate().is_none());
    assert!(request.functions[0].package.is_none());
    assert_eq!(request.secrets["GLOBAL_TOKEN"], "project-value");
    let secrets = request.functions[0].secrets.as_ref().unwrap();
    assert_eq!(secrets.scope, "payments");
    assert_eq!(secrets.values["TOKEN"], "package-value");
}

#[test]
fn accepts_package_identity_from_new_cli() {
    let mut json = legacy_request();
    json["functions"][0]["package"] = json!("payments");
    let request: Request = serde_json::from_value(json).unwrap();
    assert!(request.validate().is_none());
    assert_eq!(request.functions[0].package.as_deref(), Some("payments"));
}
