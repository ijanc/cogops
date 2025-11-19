# cogops

`cogops` is a command-line tool for performing batch operations on AWS Cognito
user pools.\
It supports synchronizing users into a local file, adding users to groups, and
removing users from groups.

## Why

Some of our internal systems rely on AWS Cognito Group membership for
authorization. However, Cognito:

- Does not support batch operations
- Requires the opaque Cognito username for group changes
- Does not allow group operations using the user’s email
- Can throttle if performing requests per user per group
- Each user lookup by email requires a full search query

### Command sync Required?

Our Cognito User Pool integrates with Google Identity Provider.

Cognito usernames look like: **Google_AbCdEf1234567890**

Emails are stored only as attributes and cannot be passed to Cognito Admin
APIs. Other internal systems only know users by email — mismatch.

Solution, the sync command downloads all users via paginated calls:

```
username,email
Google_a3be23de...,user@example.com
Google_91cfeacb...,another@example.com
```

This creates a local, up-to-date user index so later add and del operations
run:

- 1 direct request per user
- No additional lookup/search required
- No wasted API calls

## Features

- sync: Generates an optimized local mapping username,email of all Cognito
  users
- add: Add users in bulk to one or more groups
- Concurrency Control
- Operation timetout

## Requirements

- Rust toolchain (Rust 1.75 or newer recommended)
- AWS credentials with Administrator privileges for the target Cognito user
  pool
- Access to the AWS API (environment variables, credential file, or IAM role)

To install Rust:

```
curl https://sh.rustup.rs -sSf | sh
```

Verify installation:

```
rustc --version
cargo --version
```

## Building

Clone the repository and build the binary:

```
git clone https://github.com/ijanc/cogops.git
cd cogops
cargo build --release
```

The binary will be located at:

```
target/release/cogops
```

You can add it to your PATH or move it to `/usr/local/bin`.

## AWS Credentials

`cogops` uses the official AWS Rust SDK and respects all standard credential
providers.

For example:

```
export AWS_ACCESS_KEY_ID=...
export AWS_SECRET_ACCESS_KEY=...
export AWS_REGION=us-east-1
```

---

## Commands Overview

`cogops` provides three main operations:

1. `sync`\
   Export all users of a Cognito User Pool into a local CSV file containing:\
   `username,email`.

2. `add`\
   Add users (specified by email) to one or more Cognito groups.

3. `del`\
   Remove users from one or more Cognito groups.

## 1. Synchronizing users (sync)

This operation reads all users from the provided Cognito User Pool ID and
writes them to a CSV file.

Example:

```
cogops sync   --pool-id us-east-1_ABC123   --emails-file cognito_sync.csv
```

Output file format:

```
username,email
alice,alice@example.com
bob,bob@example.com
carol,carol@example.com
```

This file is later used by the `add` and `del` operations.

## 2. Adding users to groups (add)

This operation requires two input files:

1. The sync CSV file (`username,email`)
2. A text file containing one email per line

All emails will be normalized (lowercase, trim) before lookup.

Example:

```
cogops add --pool-id us-east-1_ABC123 --sync-file cognito_sync.csv \
    --emails-file to_add.txt --group admin --group managers \
    --concurrency 10
```

Where `to_add.txt` might contain:

```
alice@example.com
carol@example.com
john@example.com
```

For each email, `cogops` resolves the username from the sync map and calls the
Cognito Admin API to add the user to the specified groups.

A progress bar is displayed during processing.

## 3. Removing users from groups (del) (WIP)

This command mirrors the `add` command but removes users instead of adding
them.

Example:

```
cogops del --pool-id us-east-1_ABC123 --sync-file cognito_sync.csv \
    --emails-file to_remove.txt --group admin   --concurrency 5
```

## Logging and verbosity

`cogops` uses `tracing` for structured logging.

By default, logs are shown at the INFO level.\
Use `-v` to enable DEBUG logs:

```
cogops -v add ...
```

Or configure via `RUST_LOG`:

```
RUST_LOG=debug cogops add ...
```

## License

Licensed under ISC license ([LICENSE](LICENSE) or
https://opensource.org/licenses/ISC)
