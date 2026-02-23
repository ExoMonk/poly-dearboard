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
        );

        CREATE TABLE IF NOT EXISTS copy_trade_sessions (
            id                TEXT PRIMARY KEY,
            owner             TEXT NOT NULL,
            list_id           TEXT,
            top_n             INTEGER,
            copy_pct          REAL NOT NULL,
            max_position_usdc REAL NOT NULL DEFAULT 500.0,
            max_slippage_bps  INTEGER NOT NULL DEFAULT 200,
            order_type        TEXT NOT NULL DEFAULT 'FOK',
            initial_capital   REAL NOT NULL,
            remaining_capital REAL NOT NULL,
            simulate          INTEGER NOT NULL DEFAULT 0,
            max_loss_pct      REAL,
            status            TEXT NOT NULL DEFAULT 'running',
            created_at        TEXT NOT NULL,
            updated_at        TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS copy_trade_orders (
            id              TEXT PRIMARY KEY,
            session_id      TEXT NOT NULL,
            source_tx_hash  TEXT NOT NULL,
            source_trader   TEXT NOT NULL,
            clob_order_id   TEXT,
            asset_id        TEXT NOT NULL,
            side            TEXT NOT NULL,
            price           REAL NOT NULL,
            source_price    REAL NOT NULL,
            size_usdc       REAL NOT NULL,
            size_shares     REAL,
            status          TEXT NOT NULL DEFAULT 'pending',
            error_message   TEXT,
            fill_price      REAL,
            slippage_bps    REAL,
            tx_hash         TEXT,
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL,
            FOREIGN KEY (session_id) REFERENCES copy_trade_sessions(id) ON DELETE CASCADE
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

// ---------------------------------------------------------------------------
// Copy-Trade Sessions & Orders
// ---------------------------------------------------------------------------

pub struct CopyTradeSessionRow {
    pub id: String,
    pub owner: String,
    pub list_id: Option<String>,
    pub top_n: Option<u32>,
    pub copy_pct: f64,
    pub max_position_usdc: f64,
    pub max_slippage_bps: u32,
    pub order_type: String,
    pub initial_capital: f64,
    pub remaining_capital: f64,
    pub simulate: bool,
    pub max_loss_pct: Option<f64>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct CopyTradeOrderRow {
    pub id: String,
    pub session_id: String,
    pub source_tx_hash: String,
    pub source_trader: String,
    pub clob_order_id: Option<String>,
    pub asset_id: String,
    pub side: String,
    pub price: f64,
    pub source_price: f64,
    pub size_usdc: f64,
    pub size_shares: Option<f64>,
    pub status: String,
    pub error_message: Option<String>,
    pub fill_price: Option<f64>,
    pub slippage_bps: Option<f64>,
    pub tx_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub fn create_copytrade_session(
    conn: &Connection,
    row: &CopyTradeSessionRow,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO copy_trade_sessions
            (id, owner, list_id, top_n, copy_pct, max_position_usdc, max_slippage_bps,
             order_type, initial_capital, remaining_capital, simulate, max_loss_pct, status,
             created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
        rusqlite::params![
            row.id, row.owner, row.list_id, row.top_n, row.copy_pct,
            row.max_position_usdc, row.max_slippage_bps, row.order_type,
            row.initial_capital, row.remaining_capital, row.simulate as i32,
            row.max_loss_pct, row.status, row.created_at, row.updated_at,
        ],
    )?;
    Ok(())
}

pub fn get_copytrade_sessions(
    conn: &Connection,
    owner: &str,
) -> Result<Vec<CopyTradeSessionRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, list_id, top_n, copy_pct, max_position_usdc, max_slippage_bps,
                order_type, initial_capital, remaining_capital, simulate, max_loss_pct,
                status, created_at, updated_at
         FROM copy_trade_sessions WHERE owner = ?1 ORDER BY created_at DESC",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![owner], map_session_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_copytrade_session(
    conn: &Connection,
    id: &str,
    owner: &str,
) -> Result<Option<CopyTradeSessionRow>, rusqlite::Error> {
    conn.query_row(
        "SELECT id, owner, list_id, top_n, copy_pct, max_position_usdc, max_slippage_bps,
                order_type, initial_capital, remaining_capital, simulate, max_loss_pct,
                status, created_at, updated_at
         FROM copy_trade_sessions WHERE id = ?1 AND owner = ?2",
        rusqlite::params![id, owner],
        map_session_row,
    )
    .optional()
}

pub fn update_session_status(
    conn: &Connection,
    id: &str,
    status: &str,
) -> Result<bool, rusqlite::Error> {
    let now = chrono::Utc::now().to_rfc3339();
    let changed = conn.execute(
        "UPDATE copy_trade_sessions SET status = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![status, now, id],
    )?;
    Ok(changed > 0)
}

pub fn update_session_capital(
    conn: &Connection,
    id: &str,
    remaining: f64,
) -> Result<(), rusqlite::Error> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE copy_trade_sessions SET remaining_capital = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![remaining, now, id],
    )?;
    Ok(())
}

pub fn delete_copytrade_session(
    conn: &Connection,
    id: &str,
    owner: &str,
) -> Result<bool, rusqlite::Error> {
    let changed = conn.execute(
        "DELETE FROM copy_trade_sessions WHERE id = ?1 AND owner = ?2",
        rusqlite::params![id, owner],
    )?;
    Ok(changed > 0)
}

pub fn has_active_copytrade_session(
    conn: &Connection,
    owner: &str,
) -> Result<bool, rusqlite::Error> {
    let count: u32 = conn.query_row(
        "SELECT COUNT(*) FROM copy_trade_sessions WHERE owner = ?1 AND status IN ('running', 'paused')",
        rusqlite::params![owner],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub fn get_running_sessions(
    conn: &Connection,
) -> Result<Vec<CopyTradeSessionRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, owner, list_id, top_n, copy_pct, max_position_usdc, max_slippage_bps,
                order_type, initial_capital, remaining_capital, simulate, max_loss_pct,
                status, created_at, updated_at
         FROM copy_trade_sessions WHERE status = 'running'",
    )?;
    let rows = stmt
        .query_map([], map_session_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn insert_copytrade_order(
    conn: &Connection,
    row: &CopyTradeOrderRow,
) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO copy_trade_orders
            (id, session_id, source_tx_hash, source_trader, clob_order_id, asset_id, side,
             price, source_price, size_usdc, size_shares, status, error_message,
             fill_price, slippage_bps, tx_hash, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
        rusqlite::params![
            row.id, row.session_id, row.source_tx_hash, row.source_trader,
            row.clob_order_id, row.asset_id, row.side, row.price, row.source_price,
            row.size_usdc, row.size_shares, row.status, row.error_message,
            row.fill_price, row.slippage_bps, row.tx_hash, row.created_at, row.updated_at,
        ],
    )?;
    Ok(())
}

pub fn update_copytrade_order(
    conn: &Connection,
    id: &str,
    status: &str,
    fill_price: Option<f64>,
    slippage_bps: Option<f64>,
    tx_hash: Option<&str>,
    clob_order_id: Option<&str>,
) -> Result<(), rusqlite::Error> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE copy_trade_orders SET status = ?1, fill_price = ?2, slippage_bps = ?3,
                tx_hash = ?4, clob_order_id = ?5, updated_at = ?6 WHERE id = ?7",
        rusqlite::params![status, fill_price, slippage_bps, tx_hash, clob_order_id, now, id],
    )?;
    Ok(())
}

pub fn get_session_orders(
    conn: &Connection,
    session_id: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<CopyTradeOrderRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, source_tx_hash, source_trader, clob_order_id, asset_id, side,
                price, source_price, size_usdc, size_shares, status, error_message,
                fill_price, slippage_bps, tx_hash, created_at, updated_at
         FROM copy_trade_orders WHERE session_id = ?1
         ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![session_id, limit, offset], map_order_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get_net_shares(
    conn: &Connection,
    session_id: &str,
    asset_id: &str,
) -> Result<f64, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(
            SUM(CASE WHEN side = 'buy' AND status IN ('filled', 'simulated') THEN size_shares ELSE 0 END) -
            SUM(CASE WHEN side = 'sell' AND status IN ('filled', 'simulated') THEN size_shares ELSE 0 END),
            0.0
        ) FROM copy_trade_orders WHERE session_id = ?1 AND asset_id = ?2",
        rusqlite::params![session_id, asset_id],
        |row| row.get(0),
    )
}

/// Returns the estimated market value of open positions for a session.
/// Computes net_shares per asset × last known fill price for that asset.
pub fn get_session_positions_value(
    conn: &Connection,
    session_id: &str,
) -> Result<f64, rusqlite::Error> {
    // For each asset with a net long position, use the most recent fill_price
    // as the best available price estimate (no extra CLOB API calls needed).
    let mut stmt = conn.prepare(
        "SELECT
            o.asset_id,
            SUM(CASE WHEN o.side = 'buy' AND o.status IN ('filled', 'simulated') THEN o.size_shares ELSE 0 END) -
            SUM(CASE WHEN o.side = 'sell' AND o.status IN ('filled', 'simulated') THEN o.size_shares ELSE 0 END) AS net_shares,
            -- Last fill price for this asset (most recent order with a fill)
            (SELECT fill_price FROM copy_trade_orders
             WHERE session_id = ?1 AND asset_id = o.asset_id
               AND fill_price IS NOT NULL AND status IN ('filled', 'simulated')
             ORDER BY created_at DESC LIMIT 1) AS last_price
         FROM copy_trade_orders o
         WHERE o.session_id = ?1
         GROUP BY o.asset_id
         HAVING net_shares > 0.001",
    )?;
    let values: Result<Vec<f64>, _> = stmt
        .query_map(rusqlite::params![session_id], |row| {
            let net_shares: f64 = row.get(1)?;
            let last_price: f64 = row.get::<_, Option<f64>>(2)?.unwrap_or(0.0);
            Ok(net_shares * last_price)
        })?
        .collect();
    Ok(values?.into_iter().sum())
}

/// Returns all open positions for a session: asset_id → (net_shares, last_fill_price).
/// Used to restore in-memory positions on engine restart.
pub fn get_session_positions(
    conn: &Connection,
    session_id: &str,
) -> Result<std::collections::HashMap<String, (f64, f64)>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT
            o.asset_id,
            SUM(CASE WHEN o.side = 'buy' AND o.status IN ('filled', 'simulated') THEN o.size_shares ELSE 0 END) -
            SUM(CASE WHEN o.side = 'sell' AND o.status IN ('filled', 'simulated') THEN o.size_shares ELSE 0 END) AS net_shares,
            (SELECT fill_price FROM copy_trade_orders
             WHERE session_id = ?1 AND asset_id = o.asset_id
               AND fill_price IS NOT NULL AND status IN ('filled', 'simulated')
             ORDER BY created_at DESC LIMIT 1) AS last_price
         FROM copy_trade_orders o
         WHERE o.session_id = ?1
         GROUP BY o.asset_id
         HAVING net_shares > 0.001",
    )?;
    let rows: Result<Vec<_>, _> = stmt
        .query_map(rusqlite::params![session_id], |row| {
            let asset_id: String = row.get(0)?;
            let net_shares: f64 = row.get(1)?;
            let last_price: f64 = row.get::<_, Option<f64>>(2)?.unwrap_or(0.0);
            Ok((asset_id, (net_shares, last_price)))
        })?
        .collect();
    Ok(rows?.into_iter().collect())
}

/// Returns the last fill price for a specific asset in a session, if any.
pub fn get_last_fill_price(
    conn: &Connection,
    session_id: &str,
    asset_id: &str,
) -> Result<Option<f64>, rusqlite::Error> {
    conn.query_row(
        "SELECT fill_price FROM copy_trade_orders
         WHERE session_id = ?1 AND asset_id = ?2
           AND fill_price IS NOT NULL AND status IN ('filled', 'simulated')
         ORDER BY created_at DESC LIMIT 1",
        rusqlite::params![session_id, asset_id],
        |row| row.get(0),
    )
    .optional()
}

// ---------------------------------------------------------------------------
// Copy-Trade Dashboard (spec 16) — stats + positions queries
// ---------------------------------------------------------------------------

/// Raw order-level stats from copy_trade_orders.
/// Handler computes derived fields (win/loss, unrealized P&L, etc.)
pub struct OrderStatsRaw {
    pub total_orders: u32,
    pub filled_orders: u32,
    pub failed_orders: u32,
    pub pending_orders: u32,
    pub canceled_orders: u32,
    pub total_invested: f64,
    pub total_returned: f64,
    pub avg_slippage_bps: f64,
    pub max_slippage_bps: f64,
}

pub fn get_session_order_stats(
    conn: &Connection,
    session_id: &str,
) -> Result<OrderStatsRaw, rusqlite::Error> {
    conn.query_row(
        "SELECT
            COUNT(*) AS total_orders,
            SUM(CASE WHEN status IN ('filled','simulated') THEN 1 ELSE 0 END) AS filled_orders,
            SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failed_orders,
            SUM(CASE WHEN status IN ('pending','submitted') THEN 1 ELSE 0 END) AS pending_orders,
            SUM(CASE WHEN status = 'canceled' THEN 1 ELSE 0 END) AS canceled_orders,
            COALESCE(SUM(CASE WHEN side='buy' AND status IN ('filled','simulated') THEN size_usdc ELSE 0.0 END), 0.0) AS total_invested,
            COALESCE(SUM(CASE WHEN side='sell' AND status IN ('filled','simulated') THEN size_usdc ELSE 0.0 END), 0.0) AS total_returned,
            COALESCE(AVG(CASE WHEN slippage_bps IS NOT NULL AND status IN ('filled','simulated') THEN slippage_bps END), 0.0) AS avg_slippage,
            COALESCE(MAX(CASE WHEN slippage_bps IS NOT NULL AND status IN ('filled','simulated') THEN slippage_bps END), 0.0) AS max_slippage
         FROM copy_trade_orders WHERE session_id = ?1",
        rusqlite::params![session_id],
        |row| {
            Ok(OrderStatsRaw {
                total_orders: row.get(0)?,
                filled_orders: row.get(1)?,
                failed_orders: row.get(2)?,
                pending_orders: row.get(3)?,
                canceled_orders: row.get(4)?,
                total_invested: row.get(5)?,
                total_returned: row.get(6)?,
                avg_slippage_bps: row.get(7)?,
                max_slippage_bps: row.get(8)?,
            })
        },
    )
}

/// Raw per-asset position aggregation from copy_trade_orders.
pub struct PositionRaw {
    pub asset_id: String,
    pub buy_shares: f64,
    pub sell_shares: f64,
    pub net_shares: f64,
    pub cost_basis: f64,
    pub sell_proceeds: f64,
    pub order_count: u32,
    pub source_traders: String,
    pub last_order_at: String,
    pub last_fill_price: f64,
}

pub fn get_positions_raw(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<PositionRaw>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT
            o.asset_id,
            SUM(CASE WHEN o.side='buy'  AND o.status IN ('filled','simulated') THEN COALESCE(o.size_shares, 0.0) ELSE 0.0 END) AS buy_shares,
            SUM(CASE WHEN o.side='sell' AND o.status IN ('filled','simulated') THEN COALESCE(o.size_shares, 0.0) ELSE 0.0 END) AS sell_shares,
            SUM(CASE WHEN o.side='buy'  AND o.status IN ('filled','simulated') THEN COALESCE(o.size_shares, 0.0) ELSE 0.0 END) -
            SUM(CASE WHEN o.side='sell' AND o.status IN ('filled','simulated') THEN COALESCE(o.size_shares, 0.0) ELSE 0.0 END) AS net_shares,
            COALESCE(SUM(CASE WHEN o.side='buy'  AND o.status IN ('filled','simulated') THEN o.size_usdc ELSE 0.0 END), 0.0) AS cost_basis,
            COALESCE(SUM(CASE WHEN o.side='sell' AND o.status IN ('filled','simulated') THEN o.size_usdc ELSE 0.0 END), 0.0) AS sell_proceeds,
            COUNT(*) AS order_count,
            GROUP_CONCAT(DISTINCT o.source_trader) AS source_traders,
            MAX(o.created_at) AS last_order_at,
            (SELECT fill_price FROM copy_trade_orders
             WHERE session_id = ?1 AND asset_id = o.asset_id
               AND fill_price IS NOT NULL AND status IN ('filled', 'simulated')
             ORDER BY created_at DESC LIMIT 1) AS last_fill_price
         FROM copy_trade_orders o
         WHERE o.session_id = ?1
         GROUP BY o.asset_id
         HAVING buy_shares > 0.001",
    )?;
    let rows: Result<Vec<_>, _> = stmt
        .query_map(rusqlite::params![session_id], |row| {
            Ok(PositionRaw {
                asset_id: row.get(0)?,
                buy_shares: row.get(1)?,
                sell_shares: row.get(2)?,
                net_shares: row.get(3)?,
                cost_basis: row.get(4)?,
                sell_proceeds: row.get(5)?,
                order_count: row.get(6)?,
                source_traders: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                last_order_at: row.get::<_, Option<String>>(8)?.unwrap_or_default(),
                last_fill_price: row.get::<_, Option<f64>>(9)?.unwrap_or(0.0),
            })
        })?
        .collect();
    rows
}

/// Count total filled/simulated orders for a user across all sessions.
pub fn get_total_order_count(
    conn: &Connection,
    owner: &str,
) -> Result<u32, rusqlite::Error> {
    conn.query_row(
        "SELECT COUNT(o.id)
         FROM copy_trade_orders o
         JOIN copy_trade_sessions s ON o.session_id = s.id
         WHERE s.owner = ?1 AND o.status IN ('filled', 'simulated')",
        rusqlite::params![owner],
        |row| row.get(0),
    )
}

fn map_session_row(row: &rusqlite::Row) -> Result<CopyTradeSessionRow, rusqlite::Error> {
    Ok(CopyTradeSessionRow {
        id: row.get(0)?,
        owner: row.get(1)?,
        list_id: row.get(2)?,
        top_n: row.get(3)?,
        copy_pct: row.get(4)?,
        max_position_usdc: row.get(5)?,
        max_slippage_bps: row.get(6)?,
        order_type: row.get(7)?,
        initial_capital: row.get(8)?,
        remaining_capital: row.get(9)?,
        simulate: row.get::<_, i32>(10)? != 0,
        max_loss_pct: row.get(11)?,
        status: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

fn map_order_row(row: &rusqlite::Row) -> Result<CopyTradeOrderRow, rusqlite::Error> {
    Ok(CopyTradeOrderRow {
        id: row.get(0)?,
        session_id: row.get(1)?,
        source_tx_hash: row.get(2)?,
        source_trader: row.get(3)?,
        clob_order_id: row.get(4)?,
        asset_id: row.get(5)?,
        side: row.get(6)?,
        price: row.get(7)?,
        source_price: row.get(8)?,
        size_usdc: row.get(9)?,
        size_shares: row.get(10)?,
        status: row.get(11)?,
        error_message: row.get(12)?,
        fill_price: row.get(13)?,
        slippage_bps: row.get(14)?,
        tx_hash: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
    })
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
