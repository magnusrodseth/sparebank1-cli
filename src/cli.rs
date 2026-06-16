//! Command-line surface (clap derive).

use clap::{Args, Parser, Subcommand};

/// Feature-complete CLI for the SpareBank 1 personal banking API.
///
/// Log in once with BankID (`sb1 login`), then list accounts, browse
/// transactions, and make transfers. Credentials and tokens are stored in your
/// system keychain. Usage is personal and rate-limited per the bank's API terms.
#[derive(Debug, Parser)]
#[command(name = "sb1", version, about, long_about = None)]
pub struct Cli {
    /// Output machine-readable JSON instead of tables.
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Authenticate with BankID and store the token in the keychain.
    Login(LoginArgs),

    /// Remove the stored token (and optionally the client credentials).
    Logout {
        /// Also delete stored client id/secret from the keychain.
        #[arg(long)]
        all: bool,
    },

    /// Show authentication status and token expiry.
    Status,

    /// Verify authentication with the Hello World endpoint.
    Hello,

    /// Force an OAuth token refresh using the stored refresh token.
    Refresh,

    /// List your accounts.
    Accounts(AccountsArgs),

    /// Show a single account by key.
    Account(AccountArgs),

    /// Look up a balance by account number (POST /accounts/balance).
    Balance {
        /// Account number (digits, with or without dots).
        account_number: String,
    },

    /// List transactions for one or more accounts.
    Transactions(TxnArgs),

    /// Show details for a single transaction id.
    Transaction {
        /// Transaction id (from the `transactions` list).
        id: String,
        /// Use the classified detail endpoint.
        #[arg(long)]
        classified: bool,
    },

    /// Export booked transactions to CSV via the server-side export endpoint.
    Export(ExportArgs),

    /// Make a transfer (asks for confirmation before executing).
    Transfer {
        #[command(subcommand)]
        kind: TransferKind,
    },

    /// Financial overview: net worth, monthly cash flow, categories, subscriptions.
    Summary {
        /// Number of months to analyse (default 3).
        #[arg(long, default_value_t = 3)]
        months: i64,
    },
}

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// OAuth client id (else read from keychain, env, or .env).
    #[arg(long, env = "CLIENT_ID")]
    pub client_id: Option<String>,
    /// OAuth client secret (else read from keychain, env, or .env).
    #[arg(long, env = "CLIENT_SECRET")]
    pub client_secret: Option<String>,
    /// Registered redirect URI (default http://localhost:12345/callback).
    #[arg(long, env = "REDIRECT_URL")]
    pub redirect_uri: Option<String>,
    /// Do not persist client credentials to the keychain (token only).
    #[arg(long)]
    pub no_save_credentials: bool,
}

#[derive(Debug, Args)]
pub struct AccountsArgs {
    /// Include credit card accounts.
    #[arg(long)]
    pub credit_cards: bool,
    /// Include BSU accounts.
    #[arg(long)]
    pub bsu: bool,
    /// Include ASK accounts.
    #[arg(long)]
    pub ask: bool,
    /// Include pension accounts.
    #[arg(long)]
    pub pension: bool,
    /// Include currency accounts.
    #[arg(long)]
    pub currency: bool,
    /// Include all account types.
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct AccountArgs {
    /// Account key, name, or number.
    pub account: String,
    /// Show extended account details.
    #[arg(long)]
    pub details: bool,
    /// Show account roles.
    #[arg(long)]
    pub roles: bool,
}

#[derive(Debug, Args)]
pub struct TxnArgs {
    /// Account name, key, or number, given positionally. Repeat for multiple.
    /// Equivalent to -a/--account. Defaults to all accounts if omitted.
    #[arg(value_name = "ACCOUNT")]
    pub positional_accounts: Vec<String>,
    /// Account name, key, or number (same as positional). Repeat for multiple.
    #[arg(long = "account", short = 'a', value_name = "ACCOUNT")]
    pub flag_accounts: Vec<String>,
    /// Start date (YYYY-MM-DD). Defaults to 30 days ago.
    #[arg(long)]
    pub from: Option<String>,
    /// End date (YYYY-MM-DD). Defaults to today.
    #[arg(long)]
    pub to: Option<String>,
    /// Last N days (overrides --from/--to).
    #[arg(long)]
    pub days: Option<i64>,
    /// Maximum number of transactions.
    #[arg(long)]
    pub limit: Option<i64>,
    /// Transaction source.
    #[arg(long, value_parser = ["RECENT", "HISTORIC", "ALL"])]
    pub source: Option<String>,
    /// Use the classified transactions endpoint.
    #[arg(long)]
    pub classified: bool,
    /// Output CSV instead of a table.
    #[arg(long)]
    pub csv: bool,
    /// Write output to a file instead of stdout.
    #[arg(long, short = 'o')]
    pub output: Option<String>,
}

impl TxnArgs {
    /// All account references, combining positional args and -a/--account flags.
    pub fn account_refs(&self) -> Vec<String> {
        self.positional_accounts
            .iter()
            .chain(&self.flag_accounts)
            .cloned()
            .collect()
    }
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Account name, key, or number.
    #[arg(long, short = 'a')]
    pub account: String,
    /// Start date (YYYY-MM-DD). Defaults to 90 days ago.
    #[arg(long)]
    pub from: Option<String>,
    /// End date (YYYY-MM-DD). Defaults to today.
    #[arg(long)]
    pub to: Option<String>,
    /// Comma-separated fields to include (API default if omitted).
    #[arg(long)]
    pub fields: Option<String>,
    /// Write CSV to a file instead of stdout.
    #[arg(long, short = 'o')]
    pub output: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum TransferKind {
    /// Transfer between accounts / domestic payment (POST /transfer/debit).
    Debit(DebitArgs),
    /// Pay a credit card account (POST /transfer/creditcard/transferTo).
    Creditcard(CreditcardArgs),
    /// Transfer to a pension policy (POST /transfer/pension).
    Pension(PensionArgs),
}

#[derive(Debug, Args)]
pub struct DebitArgs {
    /// Source account (name, key, or number).
    #[arg(long)]
    pub from: String,
    /// Destination account (name, key, or number).
    #[arg(long)]
    pub to: String,
    /// Amount in NOK, e.g. 250 or 250.50.
    #[arg(long)]
    pub amount: String,
    /// Optional payment message.
    #[arg(long)]
    pub message: Option<String>,
    /// Requested due date (YYYY-MM-DD). Defaults to immediate.
    #[arg(long)]
    pub due_date: Option<String>,
    /// Skip the confirmation prompt.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct CreditcardArgs {
    /// Source account (name, key, or number).
    #[arg(long)]
    pub from: String,
    /// Credit card account id.
    #[arg(long)]
    pub credit_card_id: String,
    /// Amount in NOK.
    #[arg(long)]
    pub amount: String,
    /// Requested due date (YYYY-MM-DD).
    #[arg(long)]
    pub due_date: Option<String>,
    /// Skip the confirmation prompt.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct PensionArgs {
    /// Source account (name, key, or number).
    #[arg(long)]
    pub from: String,
    /// Pension policy number (polisenummer).
    #[arg(long)]
    pub policy_number: String,
    /// Amount in NOK.
    #[arg(long)]
    pub amount: String,
    /// Requested due date (YYYY-MM-DD).
    #[arg(long)]
    pub due_date: Option<String>,
    /// Skip the confirmation prompt.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(positional: &[&str], flags: &[&str]) -> TxnArgs {
        TxnArgs {
            positional_accounts: positional.iter().map(|s| s.to_string()).collect(),
            flag_accounts: flags.iter().map(|s| s.to_string()).collect(),
            from: None,
            to: None,
            days: None,
            limit: None,
            source: None,
            classified: false,
            csv: false,
            output: None,
        }
    }

    #[test]
    fn account_refs_empty_when_none_given() {
        assert!(args(&[], &[]).account_refs().is_empty());
    }

    #[test]
    fn account_refs_returns_positional_only() {
        assert_eq!(args(&["Brukskonto"], &[]).account_refs(), ["Brukskonto"]);
    }

    #[test]
    fn account_refs_returns_flags_only() {
        assert_eq!(args(&[], &["Sparekonto"]).account_refs(), ["Sparekonto"]);
    }

    #[test]
    fn account_refs_combines_positional_before_flags() {
        assert_eq!(
            args(&["Brukskonto"], &["Sparekonto"]).account_refs(),
            ["Brukskonto", "Sparekonto"]
        );
    }
}
