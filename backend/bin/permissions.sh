#!/bin/bash
email=$1;
kinetics_username_escaped="${email//@/AT}"
kinetics_username_escaped="${kinetics_username_escaped//./DOT}"

dir=$(dirname "$0")

aws iam put-role-policy \
    --role-name "EndpointRole${kinetics_username_escaped}DbackendDDeployDeploy" \
    --policy-name DeployResourcesPolicy \
    --policy-document file://$dir/deploy-policy.json

aws iam put-role-policy \
    --role-name EndpointRoleDbackendDUploadUpload \
    --policy-name UploadLoggingPolicy \
    --policy-document file://$dir/upload-policy.json
