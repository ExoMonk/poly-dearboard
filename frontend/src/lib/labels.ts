import type { BehavioralLabel, LabelDetails } from "../types";

export function labelTooltip(label: BehavioralLabel, d: LabelDetails): string {
  const fmtVol = (v: string) => "$" + parseFloat(v).toLocaleString("en-US", { maximumFractionDigits: 0 });
  switch (label) {
    case "sharp":
      return `${d.win_rate.toFixed(1)}% win rate on ${d.settled_count} settled, z=${d.z_score.toFixed(1)}`;
    case "specialist":
      return `${d.dominant_category_pct.toFixed(0)}% in ${d.dominant_category}${d.category_win_rate > 0 ? `, ${d.category_win_rate.toFixed(0)}% cat win rate` : ""}`;
    case "whale":
      return `${fmtVol(d.total_volume)} volume, ${fmtVol(d.avg_position_size_usd)} avg, ${d.unique_markets} markets`;
    case "degen":
      return `${d.win_rate.toFixed(1)}% win rate on ${d.settled_count} settled, ${fmtVol(d.total_volume)} volume`;
    case "market_maker":
      return `${(d.buy_sell_ratio * 100).toFixed(0)}% buy/sell balance, ${d.total_trade_count} trades, ${d.unique_markets} markets`;
    case "bot":
      return `${d.total_trade_count} trades, ${d.trades_per_market.toFixed(1)} per market`;
    case "casual":
      return `${d.total_trade_count} trades, ${fmtVol(d.total_volume)} volume`;
    case "contrarian":
      return `${d.contrarian_correct}/${d.contrarian_trades} cheap buys correct (${d.contrarian_rate.toFixed(0)}%)`;
    default:
      return "";
  }
}
