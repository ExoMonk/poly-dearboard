export function formatUsd(value: string): string {
  const num = parseFloat(value);
  if (isNaN(num)) return "$0.00";
  return "$" + num.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
}

export function formatNumber(value: string | number): string {
  const num = typeof value === "string" ? parseFloat(value) : value;
  if (isNaN(num)) return "0";
  return num.toLocaleString("en-US");
}

export function shortenAddress(addr: string): string {
  if (addr.length <= 10) return addr;
  return `${addr.slice(0, 6)}...${addr.slice(-4)}`;
}

export function timeAgo(iso: string): string {
  if (!iso) return "—";
  const d = new Date(iso.endsWith("Z") ? iso : iso + "Z");
  if (isNaN(d.getTime())) return "—";
  const now = new Date();
  const diffMs = now.getTime() - d.getTime();
  const mins = Math.floor(diffMs / 60000);
  const hrs = Math.floor(mins / 60);
  const days = Math.floor(hrs / 24);

  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  if (hrs < 24) {
    const remM = mins % 60;
    return remM > 0 ? `${hrs}h ${remM}m ago` : `${hrs}h ago`;
  }
  if (days < 30) {
    const remH = hrs % 24;
    return remH > 0 ? `${days}d ${remH}h ago` : `${days}d ago`;
  }
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
}

export function formatDate(iso: string): string {
  if (!iso) return "—";
  const d = new Date(iso.endsWith("Z") ? iso : iso + "Z");
  if (isNaN(d.getTime())) return "—";
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
}

export function formatTimestamp(iso: string): string {
  if (!iso) return "—";
  const d = new Date(iso.endsWith("Z") ? iso : iso + "Z");
  if (isNaN(d.getTime())) return "—";
  return d.toLocaleString("en-US", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function polygonscanAddress(addr: string): string {
  return `https://polygonscan.com/address/${addr}`;
}

export function polygonscanTx(hash: string): string {
  return `https://polygonscan.com/tx/${hash}`;
}

export function polymarketAddress(addr: string): string {
  return `https://polymarket.com/${addr}`;
}

export function formatHoldTime(hours: number): string {
  if (hours < 1) return "<1h";
  const days = Math.floor(hours / 24);
  const rem = Math.round(hours % 24);
  if (days === 0) return `${rem}h`;
  return rem > 0 ? `${days}d ${rem}h` : `${days}d`;
}