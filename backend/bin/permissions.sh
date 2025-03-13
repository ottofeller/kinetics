#!/bin/bash
email=$1;
no_at_email="${email/"@"/AT}"
no_at_and_dot_email="${no_at_email/"."/DOT}"

dir=$(dirname "$0")

aws iam put-role-policy \
    --role-name "EndpointRole${no_at_and_dot_email}DbackendDDeployDeploy" \
    --policy-name DeployResourcesPolicy \
    --policy-document file://$dir/deploy-policy.json;

aws iam put-role-policy \
    --role-name EndpointRoleDbackendDUploadUpload \
    --policy-name UploadLoggingPolicy \
    --policy-document file://$dir/upload-policy.json;
