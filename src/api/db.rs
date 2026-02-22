use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

use super::types::{TraderList, TraderListDetail, TraderListMember};

// ---------------------------------------------------------------------------
// Trading Wallet row type (internal, includes encrypted blobs)
// ---------------------------------------------------------------------------

pub struct TradingWalletRow {
    pub id: String,
    pub owner: String,
    pub wallet_address: String,
    pub proxy_address: Option<String>,
    pub encrypted_key: Vec<u8>,
    pub key_nonce: Vec<u8>,
    pub clob_api_key: Option<String>,
    pub clob_credentials: Option<Vec<u8>>,
    pub clob_nonce: Option<Vec<u8>>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Opens (or creates) the SQLite user database and runs migrations.
/// Panics on failure — intended to be called once at startup.
pub fn init_user_db(path: &str) -> Connection {
    if let Some(parent) = Path::new(path).parent() {
        std::fs::create_dir_all(parent).expect("failed to create data directory");
    }
    let conn = Connection::open(path).expect("failed to open SQLite user DB");

    // Enable foreign keys for CASCADE deletes on trader_list_members
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .expect("failed to enable foreign keys");

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS users (
            address     TEXT PRIMARY KEY,
            nonce       TEXT NOT NULL,
            issued_at   TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            last_login  TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS trader_lists (
            id          TEXT PRIMARY KEY,
            owner       TEXT NOT NULL,
            name        TEXT NOT NULL,
            created_at  TEXT NOT NULL,
            updated_at  TEXT NOT NULL,
            UNIQUE(owner, name)
        );

        CREATE TABLE IF NOT EXISTS trader_list_members (
            list_id     TEXT NOT NULL,
            address     TEXT NOT NULL,
            label       TEXT,
            added_at    TEXT NOT NULL,
            PRIMARY KEY (list_id, address),
            FOREIGN KEY (list_id) REFERENCES trader_lists(id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS trading_wallets (
            id              TEXT PRIMARY KEY,
            owner           TEXT NOT NULL,
            wallet_address  TEXT NOT NULL,
            proxy_address   TEXT,
            encrypted_key   BLOB NOT NULL,
            key_nonce       BLOB NOT NULL,
            clob_api_key    TEXT,
            clob_credentials BLOB,
            clob_nonce      BLOB,
            status          TEXT NOT NULL DEFAULT 'created',
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL
        )",
    )
    .expect("failed to create tables");
    tracing::info!("SQLite user DB initialized at {path}");
    conn
}

/// Returns `(nonce, issued_at)` for the given address, creating the user if needed.
pub fn get_or_create_user(
    conn: &Connection,
    address: &str,
) -> Result<(String, String), rusqlite::Error> {
    let addr = address.to_lowercase();
    let nonce = generate_nonce();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO users (address, nonce, issued_at, created_at, last_login)
         VALUES (?1, ?2, ?3, ?3, ?3)
         ON CONFLICT(address) DO UPDATE SET nonce = ?2, issued_at = ?3, last_login = ?3",
        rusqlite::params![addr, nonce, now],
    )?;

    Ok((nonce, now))
}

/// Verifies the nonce and issued_at match the stored values, then rotates the nonce.
pub fn verify_and_rotate_nonce(
    conn: &Connection,
    address: &str,
    nonce: &str,
    issued_at: &str,
) -> Result<bool, rusqlite::Error> {
    let addr = address.to_lowercase();

    let stored: Option<(String, String)> = conn
        .query_row(
            "SELECT nonce, issued_at FROM users WHERE address = ?1",
            rusqlite::params![addr],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    match stored {
        Some((stored_nonce, stored_issued_at))
            if stored_nonce == nonce && stored_issued_at == issued_at =>
        {
            let new_nonce = generate_nonce();
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "UPDATE users SET nonce = ?1, last_login = ?2 WHERE address = ?3",
                rusqlite::params![new_nonce, now, addr],
            )?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn generate_nonce() -> String {
    use rand::Rng;
    let bytes: [u8; 32] = rand::rng().random();
    hex::encode(bytes)
}

// ---------------------------------------------------------------------------
// Trader Lists
// ---------------------------------------------------------------------------

/// Typed error for list operations that need specific HTTP status codes.
pub enum ListError {
    LimitExceeded(&'static str),
    DuplicateName,
    NotFound,
    Db(rusqlite::Error),
}

impl From<rusqlite::Error> for ListError {
    fn from(e: rusqlite::Error) -> Self {
        // Detect UNIQUE constraint violation for duplicate list names
        if let rusqlite::Error::SqliteFailure(ref err, _) = e {
            if err.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE {
                return ListError::DuplicateName;
            }
        }
        ListError::Db(e)
    }
}

const MAX_LISTS_PER_USER: u32 = 20;
const MAX_MEMBERS_PER_LIST: u32 = 100;

pub fn create_trader_list(
    conn: &Connection,
    owner: &str,
    name: &str,
) -> Result<TraderList, ListError> {
    let count: u32 = conn.query_row(
        "SELECT COUNT(*) FROM trader_lists WHERE owner = ?1",
        rusqlite::params![owner],
        |row| row.get(0),
    )?;
    if count >= MAX_LISTS_PER_USER {
        return Err(ListError::LimitExceeded("Maximum 20 lists per user"));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO trader_lists (id, owner, name, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        rusqlite::params![id, owner, name, now],
    )?;

    Ok(TraderList {
        id,
        name: name.to_string(),
        member_count: 0,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn list_trader_lists(
    conn: &Connection,
    owner: &str,
) -> Result<Vec<TraderList>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT l.id, l.name, l.created_at, l.updated_at,
                (SELECT COUNT(*) FROM trader_list_members m WHERE m.list_id = l.id) AS member_count
         FROM trader_lists l
         WHERE l.owner = ?1
         ORDER BY l.created_at DESC",
    )?;

    let lists = stmt
        .query_map(rusqlite::params![owner], |row| {
            Ok(TraderList {
                id: row.get(0)?,
                name: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                member_count: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(lists)
}

/// Returns list detail with members. Returns NotFound if the list doesn't exist or isn't owned.
pub fn get_trader_list(
    conn: &Connection,
    id: &str,
    owner: &str,
) -> Result<TraderListDetail, ListError> {
    let (name, created_at, updated_at): (String, String, String) = conn
        .query_row(
            "SELECT name, created_at, updated_at FROM trader_lists WHERE id = ?1 AND owner = ?2",
            rusqlite::params![id, owner],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => ListError::NotFound,
            other => ListError::Db(other),
        })?;

    let mut stmt = conn.prepare(
        "SELECT address, label, added_at FROM trader_list_members WHERE list_id = ?1 ORDER BY added_at",
    )?;
    let members = stmt
        .query_map(rusqlite::params![id], |row| {
            Ok(TraderListMember {
                address: row.get(0)?,
                label: row.get(1)?,
                added_at: row.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(TraderListDetail {
        id: id.to_string(),
        name,
        members,
        created_at,
        updated_at,
    })
}

pub fn rename_trader_list(
    conn: &Connection,
    id: &str,
    owner: &str,
    new_name: &str,
) -> Result<(), ListError> {
    let now = chrono::Utc::now().to_rfc3339();
    let changed = conn.execute(
        "UPDATE trader_lists SET name = ?1, updated_at = ?2 WHERE id = ?3 AND owner = ?4",
        rusqlite::params![new_name, now, id, owner],
    )?;
    if changed == 0 {
        return Err(ListError::NotFound);
    }
    Ok(())
}

pub fn delete_trader_list(
    conn: &Connection,
    id: &str,
    owner: &str,
) -> Result<(), ListError> {
    let changed = conn.execute(
        "DELETE FROM trader_lists WHERE id = ?1 AND owner = ?2",
        rusqlite::params![id, owner],
    )?;
    if changed == 0 {
        return Err(ListError::NotFound);
    }
    Ok(())
}

pub fn add_list_members(
    conn: &Connection,
    list_id: &str,
    owner: &str,
    addresses: &[(String, Option<String>)],
) -> Result<(), ListError> {
    // Verify ownership
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM trader_lists WHERE id = ?1 AND owner = ?2",
            rusqlite::params![list_id, owner],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !exists {
        return Err(ListError::NotFound);
    }

    // Check member limit
    let current: u32 = conn.query_row(
        "SELECT COUNT(*) FROM trader_list_members WHERE list_id = ?1",
        rusqlite::params![list_id],
        |row| row.get(0),
    )?;
    if current + addresses.len() as u32 > MAX_MEMBERS_PER_LIST {
        return Err(ListError::LimitExceeded("Maximum 100 members per list"));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let updated_at = now.clone();

    for (addr, label) in addresses {
        conn.execute(
            "INSERT OR IGNORE INTO trader_list_members (list_id, address, label, added_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![list_id, addr, label, now],
        )?;
    }

    conn.execute(
        "UPDATE trader_lists SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![updated_at, list_id],
    )?;

    Ok(())
}

pub fn remove_list_members(
    conn: &Connection,
    list_id: &str,
    owner: &str,
    addresses: &[String],
) -> Result<(), ListError> {
    // Verify ownership
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM trader_lists WHERE id = ?1 AND owner = ?2",
            rusqlite::params![list_id, owner],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !exists {
        return Err(ListError::NotFound);
    }

    for addr in addresses {
        conn.execute(
            "DELETE FROM trader_list_members WHERE list_id = ?1 AND address = ?2",
            rusqlite::params![list_id, addr],
        )?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE trader_lists SET updated_at = ?1 WHERE id = ?2",
        rusqlite::params![now, list_id],
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Trading Wallets
// ---------------------------------------------------------------------------

pub const MAX_WALLETS_PER_USER: usize = 3;

pub fn count_trading_wallets(
    conn: &Connection,
    owner: &str,
) -> Result<usize, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(*) FROM trading_wallets WHERE owner = ?1",
        rusqlite::params![owner],
        |row| row.get(0),
    )
}

pub fn create_trading_wallet(
    conn: &Connection,
    owner: &str,
    wallet_address: &str,
    proxy_address: &str,
    encrypted_key: &[u8],
    key_nonce: &[u8],
) -> Result<String, WalletError> {
    let count = count_trading_wallets(conn, owner)?;
    if count >= MAX_WALLETS_PER_USER {
        return Err(WalletError::LimitReached);
    }

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO trading_wallets (id, owner, wallet_address, proxy_address, encrypted_key, key_nonce, status, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'created', ?7, ?7)",
        rusqlite::params![id, owner, wallet_address, proxy_address, encrypted_key, key_nonce, now],
    )?;

    Ok(id)
}

pub fn get_trading_wallets(
    conn: &Connection,
    owner: &str,
) -> Result<Vec<TradingWalletRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, wallet_address, proxy_address, encrypted_key, key_nonce,
                clob_api_key, clob_credentials, clob_nonce, status, created_at, updated_at
         FROM trading_wallets WHERE owner = ?1 ORDER BY created_at ASC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![owner], |row| {
            Ok(TradingWalletRow {
                id: row.get(0)?,
                owner: row.get(1)?,
                wallet_address: row.get(2)?,
                proxy_address: row.get(3)?,
                encrypted_key: row.get(4)?,
                key_nonce: row.get(5)?,
                clob_api_key: row.get(6)?,
                clob_credentials: row.get(7)?,
                clob_nonce: row.get(8)?,
                status: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_trading_wallet_by_id(
    conn: &Connection,
    owner: &str,
    id: &str,
) -> Result<Option<TradingWalletRow>, rusqlite::Error> {
    conn.query_row(
        "SELECT id, owner, wallet_address, proxy_address, encrypted_key, key_nonce,
                clob_api_key, clob_credentials, clob_nonce, status, created_at, updated_at
         FROM trading_wallets WHERE owner = ?1 AND id = ?2",
        rusqlite::params![owner, id],
        |row| {
            Ok(TradingWalletRow {
                id: row.get(0)?,
                owner: row.get(1)?,
                wallet_address: row.get(2)?,
                proxy_address: row.get(3)?,
                encrypted_key: row.get(4)?,
                key_nonce: row.get(5)?,
                clob_api_key: row.get(6)?,
                clob_credentials: row.get(7)?,
                clob_nonce: row.get(8)?,
                status: row.get(9)?,
                created_at: row.get(10)?,
                updated_at: row.get(11)?,
            })
        },
    )
    .optional()
}

pub fn update_wallet_credentials(
    conn: &Connection,
    owner: &str,
    wallet_id: &str,
    api_key: &str,
    cred_blob: &[u8],
    cred_nonce: &[u8],
) -> Result<(), WalletError> {
    let now = chrono::Utc::now().to_rfc3339();
    let changed = conn.execute(
        "UPDATE trading_wallets SET clob_api_key = ?1, clob_credentials = ?2, clob_nonce = ?3,
                status = 'credentialed', updated_at = ?4
         WHERE owner = ?5 AND id = ?6",
        rusqlite::params![api_key, cred_blob, cred_nonce, now, owner, wallet_id],
    )?;
    if changed == 0 {
        return Err(WalletError::NotFound);
    }
    Ok(())
}

#[allow(dead_code)]
pub fn update_wallet_status(
    conn: &Connection,
    owner: &str,
    wallet_id: &str,
    status: &str,
) -> Result<(), WalletError> {
    let now = chrono::Utc::now().to_rfc3339();
    let changed = conn.execute(
        "UPDATE trading_wallets SET status = ?1, updated_at = ?2 WHERE owner = ?3 AND id = ?4",
        rusqlite::params![status, now, owner, wallet_id],
    )?;
    if changed == 0 {
        return Err(WalletError::NotFound);
    }
    Ok(())
}

pub fn delete_trading_wallet(
    conn: &Connection,
    owner: &str,
    wallet_id: &str,
) -> Result<(), WalletError> {
    let changed = conn.execute(
        "DELETE FROM trading_wallets WHERE owner = ?1 AND id = ?2",
        rusqlite::params![owner, wallet_id],
    )?;
    if changed == 0 {
        return Err(WalletError::NotFound);
    }
    Ok(())
}

pub enum WalletError {
    LimitReached,
    NotFound,
    Db(rusqlite::Error),
}

impl From<rusqlite::Error> for WalletError {
    fn from(e: rusqlite::Error) -> Self {
        WalletError::Db(e)
    }
}

/// Returns lowercase addresses from a list. Verifies ownership. Returns NotFound if not owned.
pub fn get_list_member_addresses(
    conn: &Connection,
    list_id: &str,
    owner: &str,
) -> Result<Vec<String>, ListError> {
    let exists: bool = conn
        .query_row(
            "SELECT 1 FROM trader_lists WHERE id = ?1 AND owner = ?2",
            rusqlite::params![list_id, owner],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !exists {
        return Err(ListError::NotFound);
    }

    let mut stmt = conn.prepare(
        "SELECT address FROM trader_list_members WHERE list_id = ?1",
    )?;
    let addrs = stmt
        .query_map(rusqlite::params![list_id], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;

    Ok(addrs)
}
