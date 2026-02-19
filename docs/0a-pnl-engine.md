# PnL Engine

**The Formula**
- Per-asset position:
```
cash_flow  = sum(usdc_amount WHERE sell) - sum(usdc_amount WHERE buy)
net_tokens = sum(amount WHERE buy) - sum(amount WHERE sell)
```


- Per-trader PnL (mark-to-market):
```
PnL = SUM over all assets ( cash_flow + net_tokens × latest_price )

daily_pnl = sum(usdc WHERE sell) - sum(usdc WHERE buy) per day
cumulative_pnl = running_sum(daily_pnl)
```
Where latest_price = the most recent trade price for that asset market-wide (not per-trader).

**Walk-through Example**
Say a trader:
- Buys 1000 "Yes" tokens at $0.65 → cash_flow = -$650, net_tokens = +1000
- Sells 500 at $0.80 → cash_flow = -$650 + $400 = -$250, net_tokens = +500
- Latest market price = $0.85

PnL = -$250 + (500 × $0.85) = -$250 + $425 = +$175


## Challenge

When a market resolves and a trader redeems winning tokens:

- The redemption is a `PayoutRedemption` event (separate contract), NOT a sell trade
So our trades table still shows net_tokens > 0 after redemption. We value those tokens at latest_price (the last trade before resolution)

