#!/usr/bin/env bash
set -euo pipefail

API="http://localhost:3001/api"

# Colors
BOLD="\033[1m"
DIM="\033[2m"
CYAN="\033[36m"
GREEN="\033[32m"
YELLOW="\033[33m"
RED="\033[31m"
RESET="\033[0m"

header() {
    echo ""
    echo -e "${BOLD}${CYAN}── $1 ──${RESET}"
    echo ""
}

# Fetch JSON from API, return body (or exit section on error)
fetch() {
    local url="$1"
    echo -e "  ${DIM}GET ${url}${RESET}" >&2
    local response
    response=$(curl -s -w "\n%{http_code}" "$url")
    local http_code body
    http_code=$(echo "$response" | tail -1)
    body=$(echo "$response" | sed '$d')

    if [ "$http_code" -ge 200 ] && [ "$http_code" -lt 300 ]; then
        echo "$body"
    else
        echo -e "  ${RED}HTTP ${http_code}${RESET}" >&2
        echo "$body" | jq -r '.message // .' 2>/dev/null >&2 || echo "$body" >&2
        return 1
    fi
}

# Truncate and pad a string to exactly N chars
cell() {
    local val="$1" width="$2"
    printf "%-${width}.${width}s" "$val"
}

# Format large numbers: 1234567.89 → 1,234,567.89
fmt_num() {
    local val="$1"
    # Split on decimal point
    local int_part="${val%%.*}"
    local dec_part=""
    if [[ "$val" == *.* ]]; then
        dec_part=".${val#*.}"
        # Trim to 2 decimal places
        dec_part=$(printf "%.2s" "$dec_part" 2>/dev/null || echo "$dec_part")
        dec_part=".${dec_part#.}"
        dec_part="${dec_part:0:3}"
    fi
    # Handle negative
    local sign=""
    if [[ "$int_part" == -* ]]; then
        sign="-"
        int_part="${int_part#-}"
    fi
    # Add commas
    local formatted=""
    local len=${#int_part}
    local i=0
    for (( i=0; i<len; i++ )); do
        if (( i > 0 && (len - i) % 3 == 0 )); then
            formatted="${formatted},"
        fi
        formatted="${formatted}${int_part:$i:1}"
    done
    echo "${sign}${formatted}${dec_part}"
}

# ── Health ───────────────────────────────────────────────────────────────────
header "Health Check"
health=$(fetch "$API/health") || { echo "  API not reachable. Is 'make serve' running?"; exit 1; }

trade_count=$(echo "$health" | jq -r '.trade_count')
trader_count=$(echo "$health" | jq -r '.trader_count')
latest_block=$(echo "$health" | jq -r '.latest_block')

echo -e "  Status:        ${GREEN}$(echo "$health" | jq -r '.status')${RESET}"
echo -e "  Trades:        ${BOLD}$(fmt_num "$trade_count")${RESET}"
echo -e "  Traders:       ${BOLD}$(fmt_num "$trader_count")${RESET}"
echo -e "  Latest Block:  ${BOLD}$(fmt_num "$latest_block")${RESET}"

# ── Leaderboard table formatter ──────────────────────────────────────────────
print_leaderboard() {
    local json="$1" highlight_col="$2"

    # Table header
    printf "  ${DIM}%-4s  %-14s  %14s  %14s  %8s  %6s${RESET}\n" \
        "#" "Address" "Realized PnL" "Volume" "Trades" "Mkts"
    printf "  ${DIM}%-4s  %-14s  %14s  %14s  %8s  %6s${RESET}\n" \
        "----" "--------------" "--------------" "--------------" "--------" "------"

    local count
    count=$(echo "$json" | jq '.traders | length')

    for (( i=0; i<count; i++ )); do
        local addr pnl vol trades mkts
        addr=$(echo "$json" | jq -r ".traders[$i].address")
        pnl=$(echo "$json" | jq -r ".traders[$i].realized_pnl")
        vol=$(echo "$json" | jq -r ".traders[$i].total_volume")
        trades=$(echo "$json" | jq -r ".traders[$i].trade_count")
        mkts=$(echo "$json" | jq -r ".traders[$i].markets_traded")

        # Shorten address: 0x1234...abcd
        local short_addr="${addr:0:6}...${addr: -4}"

        # Format numbers
        local fmt_pnl fmt_vol fmt_trades fmt_mkts
        fmt_pnl=$(fmt_num "$pnl")
        fmt_vol=$(fmt_num "$vol")
        fmt_trades=$(fmt_num "$trades")
        fmt_mkts=$(fmt_num "$mkts")

        # Color the highlighted column
        local pnl_str vol_str trades_str mkts_str
        case "$highlight_col" in
            pnl)     pnl_str="${GREEN}$(printf "%14s" "$fmt_pnl")${RESET}" ;;
            *)       pnl_str="$(printf "%14s" "$fmt_pnl")" ;;
        esac
        case "$highlight_col" in
            volume)  vol_str="${GREEN}$(printf "%14s" "$fmt_vol")${RESET}" ;;
            *)       vol_str="$(printf "%14s" "$fmt_vol")" ;;
        esac
        case "$highlight_col" in
            trades)  trades_str="${GREEN}$(printf "%8s" "$fmt_trades")${RESET}" ;;
            *)       trades_str="$(printf "%8s" "$fmt_trades")" ;;
        esac
        mkts_str="$(printf "%6s" "$fmt_mkts")"

        printf "  %-4s  %-14s  %b  %b  %b  %s\n" \
            "$((i+1))." "$short_addr" "$pnl_str" "$vol_str" "$trades_str" "$mkts_str"
    done
}

# ── Leaderboard: Top 10 by PnL ──────────────────────────────────────────────
header "Top 10 by Realized PnL"
lb_pnl=$(fetch "$API/leaderboard?sort=realized_pnl&order=desc&limit=10") && \
    print_leaderboard "$lb_pnl" "pnl"

# ── Leaderboard: Top 10 by Volume ───────────────────────────────────────────
header "Top 10 by Volume"
lb_vol=$(fetch "$API/leaderboard?sort=total_volume&order=desc&limit=10") && \
    print_leaderboard "$lb_vol" "volume"

# ── Leaderboard: Top 10 by Trade Count ──────────────────────────────────────
header "Top 10 by Trade Count"
lb_trades=$(fetch "$API/leaderboard?sort=trade_count&order=desc&limit=10") && \
    print_leaderboard "$lb_trades" "trades"

# ── Trader Detail ────────────────────────────────────────────────────────────
header "Trader Detail (top PnL trader)"

TOP_TRADER=$(echo "$lb_pnl" | jq -r '.traders[0].address // empty' 2>/dev/null)

if [ -n "$TOP_TRADER" ]; then
    short="${TOP_TRADER:0:6}...${TOP_TRADER: -4}"
    echo -e "  Address: ${BOLD}${TOP_TRADER}${RESET}"
    echo ""

    detail=$(fetch "$API/trader/$TOP_TRADER") && {
        echo -e "  Realized PnL:  ${GREEN}$(fmt_num "$(echo "$detail" | jq -r '.realized_pnl')")${RESET}"
        echo -e "  Total Volume:  $(fmt_num "$(echo "$detail" | jq -r '.total_volume')")"
        echo -e "  Total Fees:    $(fmt_num "$(echo "$detail" | jq -r '.total_fees')")"
        echo -e "  Trade Count:   $(fmt_num "$(echo "$detail" | jq -r '.trade_count')")"
        echo -e "  Markets:       $(echo "$detail" | jq -r '.markets_traded')"
        echo -e "  First Trade:   $(echo "$detail" | jq -r '.first_trade')"
        echo -e "  Last Trade:    $(echo "$detail" | jq -r '.last_trade')"
    }

    # ── Recent Trades ────────────────────────────────────────────────────────
    header "Recent Trades for $short"
    trades_json=$(fetch "$API/trader/$TOP_TRADER/trades?limit=5") && {
        printf "  ${DIM}%-12s  %-19s  %-10s  %-4s  %12s  %10s  %12s  %10s${RESET}\n" \
            "Block" "Timestamp" "Exchange" "Side" "Amount" "Price" "USDC" "Fee"
        printf "  ${DIM}%-12s  %-19s  %-10s  %-4s  %12s  %10s  %12s  %10s${RESET}\n" \
            "------------" "-------------------" "----------" "----" "------------" "----------" "------------" "----------"

        tcount=$(echo "$trades_json" | jq '.trades | length')

        for (( i=0; i<tcount; i++ )); do
            blk=$(echo "$trades_json" | jq -r ".trades[$i].block_number")
            ts=$(echo "$trades_json" | jq -r ".trades[$i].block_timestamp")
            exch=$(echo "$trades_json" | jq -r ".trades[$i].exchange")
            side=$(echo "$trades_json" | jq -r ".trades[$i].side")
            amt=$(echo "$trades_json" | jq -r ".trades[$i].amount")
            price=$(echo "$trades_json" | jq -r ".trades[$i].price")
            usdc=$(echo "$trades_json" | jq -r ".trades[$i].usdc_amount")
            fee=$(echo "$trades_json" | jq -r ".trades[$i].fee")

            # Color buy/sell
            if [ "$side" = "buy" ]; then
                side_str="${GREEN}buy ${RESET}"
            else
                side_str="${RED}sell${RESET}"
            fi

            printf "  %-12s  %-19s  %-10s  %b  %12s  %10s  %12s  %10s\n" \
                "$(fmt_num "$blk")" "$ts" "$exch" "$side_str" "$(fmt_num "$amt")" "$(fmt_num "$price")" "$(fmt_num "$usdc")" "$(fmt_num "$fee")"
        done
    }
else
    echo "  No traders found yet. Wait for rindexer to index some blocks."
fi

echo ""
echo -e "${GREEN}Done.${RESET}"
