#!/bin/bash
# setup envs based on tag passed
tagnet=$1
echo $tagnet
# case match is not RegEx, but wildcards/globs
case "$tagnet" in
v*-pre.*)
  echo "esme"
  export TAIJI_NETWORK=esme
  export TARI_NETWORK_DIR=testnet
  ;;
v*-rc.*)
  echo "nextnet" 
  export TAIJI_NETWORK=nextnet
  export TARI_NETWORK_DIR=nextnet
  ;;
*)
  echo "mainnet"
  export TAIJI_NETWORK=mainnet
  export TARI_NETWORK_DIR=mainnet
  ;;
esac
