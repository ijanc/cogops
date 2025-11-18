//
// Copyright (c) 2025 murilo ijanc' <murilo@ijanc.org>
//
// Permission to use, copy, modify, and distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
//

// accept groups name
// accept file with emails
// accept operation
// accept concorrency or size group == 1 -> size email se nao uso o grupo
// accept timeout
// accept poolid


mod helper;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{ArgAction, Parser, Subcommand};
use tracing::{debug, info};
use tracing_subscriber::EnvFilter;
use tokio::fs::File;
use tokio::io::{self, AsyncWrite, AsyncWriteExt};
use aws_sdk_cognitoidentityprovider::Client as CognitoClient;
use aws_sdk_cognitoidentityprovider::types::UserType;


const LONG_VERSION: &str = concat!(
    env!("CARGO_PKG_NAME"),
    " ",
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("GIT_HASH", "unknown"),
    " ",
    env!("BUILD_DATE", "unknown"),
    ")",
);

/// Batch operations for AWS Cognito user pools.
#[derive(Debug, Parser)]
#[command(
    name = "batch-cognito",
    about = "Batch operations for AWS Cognito user pools",
    version = env!("CARGO_PKG_VERSION"),
    long_version = LONG_VERSION,
    author,
    propagate_version = true
)]
struct Cli {
    /// Increase verbosity (use -v, -vv, ...).
    ///
    /// When no RUST_LOG is set, a single -v switches the log level to DEBUG.
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

/// Available batch operations.
///
/// These map directly to high-level Cognito workflows:
/// - sync: synchronize users from a source into Cognito.
/// - add: add users to one or more Cognito groups.
/// - del: remove users from one or more Cognito groups.
#[derive(Debug, Subcommand)]
enum Commands {
    /// Synchronize users with a Cognito user pool.
    Sync(SyncArgs),

    /// Add users to one or more Cognito groups.
    Add(GroupOperationArgs),

    /// Remove users from one or more Cognito groups.
    Del(GroupOperationArgs),
}

/// Common arguments shared by group-based operations.
#[derive(clap::Args, Debug, Clone)]
pub struct CommonOperationArgs {
    /// Cognito User Pool ID to operate on.
    #[arg(long = "pool-id", env = "COGNITO_USER_POOL_ID")]
    pub pool_id: String,

    /// File path used by the operation.
    /// For `sync`, this is the output file where usernames and emails are stored as CSV.
    #[arg(
        short = 'f',
        long = "file",
        value_name = "PATH",
        help = "File path. For `sync`, this is the output CSV file."
    )]
    pub emails_file: Option<PathBuf>,

    /// Maximum duration (in seconds) allowed for the operation.
    #[arg(long = "timeout", value_name = "SECONDS")]
    pub timeout: Option<u64>,

    /// Concurrency level for operations that need it.
    #[arg(long = "concurrency", value_name = "N", default_value_t = 1)]
    pub concurrency: usize,
}

/// Arguments for the `sync` operation.
#[derive(Debug, Parser)]
struct SyncArgs {
    /// Cognito User Pool ID (e.g. us-east-1_XXXXXXXXX).
    #[arg(long = "pool-id")]
    pool_id: String,

    /// Optional file containing user e-mails, one per line.
    ///
    /// Depending on the design, this can represent the source of truth
    /// to be synchronized with the Cognito user pool.
    #[arg(long = "emails-file")]
    emails_file: Option<PathBuf>,

    /// Optional list of Cognito group names used during synchronization.
    ///
    /// These can be used to ensure users are added/removed from specific
    /// groups during the sync process.
    #[arg(long = "group", alias = "groups")]
    groups: Vec<String>,

    /// Maximum number of concurrent operations.
    #[arg(long)]
    concurrency: Option<usize>,

    /// Global timeout for the sync operation, in seconds.
    #[arg(long)]
    timeout: Option<u64>,
}

/// Arguments shared by `add` and `del` group operations.
#[derive(Debug, Parser)]
struct GroupOperationArgs {
    /// Cognito User Pool ID (e.g. us-east-1_XXXXXXXXX).
    #[arg(long = "pool-id")]
    pool_id: String,

    /// One or more Cognito group names.
    ///
    /// All users found in the input file will be added to or removed from
    /// these groups, depending on the chosen subcommand.
    #[arg(long = "group", alias = "groups")]
    groups: Vec<String>,

    /// File containing user e-mails, one per line.
    ///
    /// Every e-mail read from this file will be processed for the
    /// selected group operation.
    #[arg(long = "emails-file")]
    emails_file: PathBuf,

    /// Maximum number of concurrent operations.
    #[arg(long)]
    concurrency: Option<usize>,

    /// Global timeout for the operation, in seconds.
    #[arg(long)]
    timeout: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    debug!("parsed CLI arguments: {cli:?}");

    match cli.command {
        Commands::Sync(args) => {
            let common = CommonOperationArgs {
                pool_id: args.pool_id,
                emails_file: args.emails_file,
                concurrency: 1, //args.concurrency,
                timeout: args.timeout,
            };

            run_sync(&common).await?;
        }
        _ => unimplemented!(),
        // Commands::Add(args) => {
        //     let common = CommonOperationArgs {
        //         pool_id: args.pool_id,
        //         groups: args.groups,
        //         emails_file: Some(args.emails_file),
        //         concurrency: args.concurrency,
        //         timeout: args.timeout,
        //     };

        //     run_add_groups(common).await?;
        // }
        // Commands::Del(args) => {
        //     let common = CommonOperationArgs {
        //         pool_id: args.pool_id,
        //         groups: args.groups,
        //         emails_file: Some(args.emails_file),
        //         concurrency: args.concurrency,
        //         timeout: args.timeout,
        //     };

        //     run_remove_groups(common).await?;
        // }
    }

    Ok(())
}

/// Initialize tracing based on RUST_LOG and the CLI verbosity.
///
/// Rules:
/// - If RUST_LOG is set, it is fully respected.
/// - If RUST_LOG is not set and verbose == 0 -> INFO level.
/// - If RUST_LOG is not set and verbose  > 0 -> DEBUG level.
fn init_tracing(verbose: u8) {
    if std::env::var_os("RUST_LOG").is_some() {
        tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
        return;
    }

    let filter = if verbose > 0 {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::fmt().with_env_filter(filter).init();
}

/// Synchronize Cognito users from a given user pool into a local CSV file.
///
/// The CSV format is:
/// ```text
/// username,email
/// user1@example.com,user1@example.com
/// user2@example.com,user2@example.com
/// ...
/// ```
///
/// Source of truth is Cognito: this command dumps all users from the pool.
///
/// Behavior:
/// - Paginates over all Cognito users in the pool.
/// - Extracts the `username` field and the `email` attribute (if present).
/// - Writes the data as `username,email` to the given output file or stdout.
/// - Respects the optional `timeout` passed in `CommonOperationArgs`.
pub async fn run_sync(args: &CommonOperationArgs) -> Result<()> {
    info!(
        pool_id = %args.pool_id,
        "Starting users sync from Cognito user pool"
    );

    let config = aws_config::load_from_env()
        .await;
        // .context("failed to load AWS configuration")?;
    let client = CognitoClient::new(&config);

    let timeout = args.timeout.map(Duration::from_secs);

    let sync_future = sync_users_to_csv(&client, args);

    if let Some(duration) = timeout {
        match tokio::time::timeout(duration, sync_future).await {
            Ok(result) => {
                result?;
            }
            Err(_) => {
                return Err(anyhow::anyhow!(
                    "sync operation timed out after {:?}",
                    duration
                ));
            }
        }
    } else {
        sync_future.await?;
    }

    info!("Users sync completed successfully");
    Ok(())
}

async fn run_add_groups(args: CommonOperationArgs) -> Result<()> {
    info!(
        pool_id = %args.pool_id,
        emails_file = ?args.emails_file,
        concurrency = ?args.concurrency,
        timeout = ?args.timeout,
        "add groups operation requested (not implemented yet)"
    );

    if let Some(seconds) = args.timeout {
        let _timeout = Duration::from_secs(seconds);
        debug!(?seconds, "add operation timeout configured");
    }

    // TODO: implement add-to-groups logic.
    Ok(())
}

async fn run_remove_groups(args: CommonOperationArgs) -> Result<()> {
    info!(
        pool_id = %args.pool_id,
        emails_file = ?args.emails_file,
        concurrency = ?args.concurrency,
        timeout = ?args.timeout,
        "remove groups operation requested (not implemented yet)"
    );

    if let Some(seconds) = args.timeout {
        let _timeout = Duration::from_secs(seconds);
        debug!(?seconds, "remove operation timeout configured");
    }

    // TODO: implement remove-from-groups logic.
    Ok(())
}

/// Fetch all users from Cognito and write `username,email` to a CSV destination.
///
/// If `args.emails_file` is set, the CSV is written to that file.
/// Otherwise, the CSV is written to stdout.
pub(crate)async fn sync_users_to_csv(client: &CognitoClient, args: &CommonOperationArgs) -> Result<()> {
    let mut writer: Box<dyn AsyncWrite + Unpin + Send> = if let Some(path) = &args.emails_file {
        let file = File::create(path)
            .await
            .with_context(|| format!("failed to create output file at '{}'", path.display()))?;
        Box::new(file)
    } else {
        Box::new(io::stdout())
    };

    // CSV header
    writer
        .write_all(b"username,email\n")
        .await
        .context("failed to write CSV header")?;

    let mut total_users = 0usize;
    let mut pagination_token: Option<String> = None;

    loop {
        let mut request = client
            .list_users()
            .user_pool_id(&args.pool_id)
            // 60 is the documented default max page size for Cognito ListUsers.
            .limit(60);

        if let Some(ref token) = pagination_token {
            request = request.pagination_token(token);
        }

        let response = request
            .send()
            .await
            .context("failed to call Cognito ListUsers")?;

        for user in response.users() {
            let (username, email) = extract_username_and_email(user);

            // If you prefer to skip users without email, you can check `email.is_empty()`.
            let line = format!("{username},{email}\n");
            writer
                .write_all(line.as_bytes())
                .await
                .context("failed to write CSV row")?;

            total_users += 1;
        }

        pagination_token = response
            .pagination_token()
            .map(|token| token.to_owned());

        if pagination_token.is_none() {
            break;
        }
    }

    writer.flush().await.context("failed to flush writer")?;

    info!(total_users, "Finished exporting Cognito users to CSV");
    Ok(())
}

/// Extract the `username` and `email` attribute from a Cognito `UserType`.
fn extract_username_and_email(user: &UserType) -> (String, String) {
    let username = user.username().unwrap_or_default().to_string();

    let email = user
        .attributes()
        .iter()
        .find(|attr| attr.name() == "email")
        .and_then(|attr| attr.value())
        .unwrap_or_default()
        .to_string();

    (username, email)
}

