#!/bin/bash
echo "Initializing environment variables..."

if [ ! -f "backend/local.env" ]; then
    touch backend/local.env
    echo "KINETICS_USE_PRODUCTION_DOMAIN=false" >> backend/local.env

    read -r -p "Enter KINETICS_USERNAME: " kinetics_username
    echo "KINETICS_USERNAME=\"$kinetics_username\"" >> backend/local.env

    # Replace @ to AT and . to DOT in $kinetics_username
    kinetics_username_escaped=${kinetics_username//@/AT}
    kinetics_username_escaped=${kinetics_username_escaped//./DOT}

    echo "KINETICS_USERNAME_ESCAPED=\"$kinetics_username_escaped\"" >> backend/local.env
fi

# Get KMS key
if ! grep -q "KINETICS_KMS_KEY_ID" backend/local.env; then
  KMS_KEY_ID=$(aws kms list-aliases --query "Aliases[?AliasName=='alias/aws/ssm'].TargetKeyId" --output text)

  if [ -z "$KMS_KEY_ID" ] || [ "$KMS_KEY_ID" == "None" ]; then
      echo "Error: Could not find KMS key for aws/ssm."
      echo "Please check AWS console > AWS managed keys > aws/ssm"
      exit 1
  fi

  echo "KINETICS_KMS_KEY_ID=$KMS_KEY_ID" >> backend/local.env
fi

# Create a new S3 bucket
if ! grep -q "KINETICS_S3_BUCKET_NAME" backend/local.env; then
  BUCKET_NAME="kinetics-builds-$(date +%s)-$(openssl rand -hex 4)"
  echo "Creating S3 bucket: $BUCKET_NAME..."
  aws s3 mb "s3://$BUCKET_NAME"
  echo "KINETICS_S3_BUCKET_NAME=$BUCKET_NAME" >> backend/local.env
fi

# Get the latest Cloudfront domain name
if ! grep -q "KINETICS_API_BASE" backend/local.env; then
  echo "Getting latest CloudFront domain name..."
  CLOUDFRONT_DOMAIN=$(aws cloudfront list-distributions --query "DistributionList.Items[*].[DomainName,Status,LastModifiedTime]" --output json | \
                      jq -r 'sort_by(.[2]) | reverse | .[0][0]')

  if [ -z "$CLOUDFRONT_DOMAIN" ] || [ "$CLOUDFRONT_DOMAIN" == "null" ]; then
      echo "Error: Could not retrieve CloudFront domain name."
      echo "Please deploy backend with \`cargo run -p kinetics-cli deploy --is-directly\`"
      exit 1
  fi

  echo "KINETICS_API_BASE=https://$CLOUDFRONT_DOMAIN" >> backend/local.env
fi

source backend/local.env
echo "Environment initialized successfully!"
echo "KINETICS_API_BASE: $KINETICS_API_BASE"
