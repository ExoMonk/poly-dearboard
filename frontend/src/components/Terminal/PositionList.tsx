import { useState } from "react";
import { Link } from "react-router-dom";
import { useSessionPositions, useClosePosition, useRedeemPosition } from "../../hooks/useCopyTrade";
import type { CopyTradePosition } from "../../types";

function PositionRow({ position, sessionId, canClose }: {
  position: CopyTradePosition;
  sessionId: string;
  canClose: boolean;
}) {
  const close = useClosePosition();
  const redeem = useRedeemPosition();
  const [confirming, setConfirming] = useState(false);
  const [redeemConfirming, setRedeemConfirming] = useState(false);

  const handleClose = () => {
    if (!confirming) {
      setConfirming(true);
      return;
    }
    close.mutate(
      { sessionId, assetId: position.asset_id },
      { onSettled: () => setConfirming(false) }
    );
  };

  const handleRedeem = () => {
    if (!redeemConfirming) {
      setRedeemConfirming(true);
      return;
    }
    redeem.mutate(
      { sessionId, assetId: position.asset_id },
      { onSettled: () => setRedeemConfirming(false) }
    );
  };

  const pnl = position.unrealized_pnl + position.realized_pnl;
  const isClosed = position.net_shares < 0.01;

  return (
    <div className={`flex items-center gap-2 px-2 py-1.5 text-xs border-b border-[var(--border-subtle)] last:border-b-0 hover:bg-[var(--surface-2)]/50 ${isClosed ? "opacity-50" : ""}`}>
      <Link to={`/market/${position.asset_id}`} className="flex-1 min-w-0 hover:opacity-80">
        <div className="font-medium text-blue-400 hover:underline truncate" title={position.question}>
          {position.resolved && <span className="text-[10px] text-yellow-500 mr-1">[RESOLVED]</span>}
          {position.question || position.asset_id.slice(0, 16) + "..."}
        </div>
        <div className="text-[10px] text-[var(--text-muted)]">
          {position.outcome}{position.category ? ` \u00B7 ${position.category}` : ""}
        </div>
      </Link>
      <div className="text-right w-16">
        <div className="font-mono">{position.net_shares.toFixed(2)}</div>
        <div className="text-[10px] text-[var(--text-muted)]">shares</div>
      </div>
      <div className="text-right w-16">
        <div className="font-mono">${position.current_price.toFixed(3)}</div>
        <div className="text-[10px] text-[var(--text-muted)]">@ ${position.avg_entry_price.toFixed(3)}</div>
      </div>
      <div className="text-right w-20">
        <div className="font-mono">${position.current_value.toFixed(2)}</div>
        <div className="text-[10px] text-[var(--text-muted)]">cost ${position.cost_basis.toFixed(2)}</div>
      </div>
      <div className={`text-right w-20 font-mono ${pnl >= 0 ? "text-green-400" : "text-red-400"}`}>
        {pnl >= 0 ? "+" : ""}{pnl.toFixed(2)}
      </div>
      {canClose && !isClosed && (
        position.resolved ? (
          <button
            className={`px-2 py-0.5 text-[10px] rounded border ${
              redeemConfirming
                ? "bg-yellow-500/20 text-yellow-400 border-yellow-500/30"
                : "bg-yellow-500/10 text-yellow-500 border-yellow-500/20 hover:text-yellow-300"
            }`}
            onClick={handleRedeem}
            onBlur={() => setRedeemConfirming(false)}
            disabled={redeem.isPending}
          >
            {redeem.isPending ? "..." : redeemConfirming ? "Confirm?" : "Redeem"}
          </button>
        ) : (
          <button
            className={`px-2 py-0.5 text-[10px] rounded border ${
              confirming
                ? "bg-red-500/20 text-red-400 border-red-500/30"
                : "bg-neutral-500/20 text-neutral-400 border-neutral-500/30 hover:text-neutral-300"
            }`}
            onClick={handleClose}
            onBlur={() => setConfirming(false)}
            disabled={close.isPending}
          >
            {close.isPending ? "..." : confirming ? "Confirm?" : "Close"}
          </button>
        )
      )}
    </div>
  );
}

export function PositionList({ sessionId, canClose }: { sessionId: string; canClose: boolean }) {
  const { data: positions, isLoading } = useSessionPositions(sessionId);

  if (isLoading) {
    return <div className="text-xs text-[var(--text-muted)] px-2 py-2">Loading positions...</div>;
  }

  if (!positions || positions.length === 0) {
    return <div className="text-xs text-[var(--text-muted)] px-2 py-2">No open positions</div>;
  }

  return (
    <div className="mt-2 border border-[var(--border-subtle)] rounded overflow-hidden">
      <div className="flex items-center gap-2 px-2 py-1 text-[10px] text-[var(--text-muted)] uppercase tracking-wider bg-[var(--surface-2)]/50">
        <div className="flex-1">Market</div>
        <div className="w-16 text-right">Shares</div>
        <div className="w-16 text-right">Price</div>
        <div className="w-20 text-right">Value</div>
        <div className="w-20 text-right">P&L</div>
        {canClose && <div className="w-16" />}
      </div>
      {positions.map((p) => (
        <PositionRow key={p.asset_id} position={p} sessionId={sessionId} canClose={canClose} />
      ))}
    </div>
  );
}
