use alloy::network::EthereumWallet;
use alloy::primitives::{Address, U256, address};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;

/// USDC.e on Polygon (6 decimals)
pub const USDC_ADDRESS: Address = address!("2791Bca1f2de4661ED88A30C99A7a9449Aa84174");

/// Polymarket CTF Exchange (binary markets)
pub const CTF_EXCHANGE: Address = address!("4bFb41d5B3570DeFd03C39a9A4D8dE6Bd8B8982E");

/// Polymarket NegRisk CTF Exchange (multi-outcome markets)
pub const NEG_RISK_EXCHANGE: Address = address!("C5d563A36AE78145C45a50134d48A1215220f80a");

pub const USDC_DECIMALS: u32 = 6;

/// Minimum POL balance required for gas (0.005 POL = 5e15 wei)
pub const MIN_POL_WEI: U256 = U256::from_limbs([5_000_000_000_000_000u64, 0, 0, 0]);

/// USDC balance below which we warn (10 USDC = 10e6 raw)
pub const LOW_BALANCE_RAW: U256 = U256::from_limbs([10_000_000u64, 0, 0, 0]);

alloy::sol! {
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
        function allowance(address owner, address spender) external view returns (uint256);
        function approve(address spender, uint256 amount) external returns (bool);
    }
}

/// Creates a read-only provider (no signer) for RPC queries.
pub fn create_provider(erpc_url: &str) -> impl Provider + Clone {
    ProviderBuilder::new().connect_http(erpc_url.parse().expect("invalid eRPC URL"))
}

/// Creates a provider with a signing wallet for sending transactions.
pub fn create_wallet_provider(signer: PrivateKeySigner, erpc_url: &str) -> impl Provider + Clone {
    let wallet = EthereumWallet::from(signer);
    ProviderBuilder::new()
        .wallet(wallet)
        .connect_http(erpc_url.parse().expect("invalid eRPC URL"))
}

/// Formats a U256 raw amount to a human-readable decimal string (e.g. "1250.50").
pub fn format_usdc(raw: U256) -> String {
    let divisor = U256::from(10u64.pow(USDC_DECIMALS));
    let whole = raw / divisor;
    let frac = raw % divisor;
    format!(
        "{}.{:0>width$}",
        whole,
        frac,
        width = USDC_DECIMALS as usize
    )
}

/// Formats a U256 wei amount to human-readable POL (18 decimals, truncated to 4).
pub fn format_pol(wei: U256) -> String {
    let divisor = U256::from(10u64.pow(18));
    let whole = wei / divisor;
    // Show 4 decimal places
    let frac_divisor = U256::from(10u64.pow(14)); // 18 - 4 = 14
    let frac = (wei % divisor) / frac_divisor;
    format!("{}.{:0>4}", whole, frac)
}
