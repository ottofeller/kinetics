#!/bin/bash
email=$1;
kinetics_username_escaped="${email//@/AT}"
kinetics_username_escaped="${kinetics_username_escaped//./DOT}"

dir=$(dirname "$0")
crate_name=backend
stack_name="$kinetics_username_escaped-$crate_name"

aws iam put-role-policy \
    --role-name $(aws cloudformation describe-stack-resource \
        --stack-name $stack_name \
        --logical-resource-id "EndpointRole${kinetics_username_escaped}DbackendDDeployDeploy" | \
        jq -r .StackResourceDetail.PhysicalResourceId) \
    --policy-name DeployResourcesPolicy \
    --policy-document file://$dir/deploy-policy.json

aws iam put-role-policy \
    --role-name $(aws cloudformation describe-stack-resource \
        --stack-name $stack_name \
        --logical-resource-id "EndpointRoleDbackendDUploadUpload" | \
        jq -r .StackResourceDetail.PhysicalResourceId) \
    --policy-name UploadLoggingPolicy \
    --policy-document file://$dir/upload-policy.json
