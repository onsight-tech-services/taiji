# Point Release checklist

THings to do before pushing a new commit to `master`:

* Create new `rc` branch off development.
* Update crate version numbers
* Check that all tests pass in development (`cargo test`, `cargo test --release`)
* Publish new crates to crates.io (`./scripts/publish_crates.sh`)
  * Fix any issues with publishing
* Rebase onto master (from rc branch, `git reset --soft master` and `git commit`)
* Tag commit
* Write release notes on GitHub.
* Merge back into development (where appropriate)
* Delete branch

| Crate                        | Version | Last change                              |
|:-----------------------------|:--------|:-----------------------------------------|
| infrastructure/derive        | 0.0.10  | 7d734a2e79bfe2dd5d4ae00a2b760614d21e69c4 |
| infrastructure/shutdown      | 0.2.3  |  |
| infrastructure/storage       | 0.2.1   | 68a59fd9a54201b2955f8a8924a63c6b402d9df3 |
| infrastructure/test_utils    | 0.2.1   | 26951ddbada794d637c740a8ea4f84057ccdc7a2 |
| base_layer/core              | 0.2.5   |  |
| base_layer/key_manager       | 0.2.1   | 68a59fd9a54201b2955f8a8924a63c6b402d9df3 |
| base_layer/mmr               | 0.2.1   | 68a59fd9a54201b2955f8a8924a63c6b402d9df3 |
| base_layer/p2p               | 0.2.3   |  |
| base_layer/service_framework | 0.2.3   |  |
| base_layer/wallet            | 0.2.5   |  |
| base_layer/wallet_ffi        | 0.16.3 |  |
| common                       | 0.2.5   |  |
| comms                        | 0.2.5   |  |
| comms/dht                    | 0.2.5   |  |
| applications/taiji_base_node  | 0.5.4   |  |
| applications/taiji_app_grpc   | 0.5.2 |  | 
| applications/taiji_console_wallet | 0.5.0 |  | 
| applications/taiji_merge_mining_proxy | 0.5.4 |  |
