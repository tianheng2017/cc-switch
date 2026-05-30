import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { Clock3, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { settingsApi } from "@/lib/api";
import { usageKeys } from "@/lib/query/usage";

export function RouterTeamUsageIntervalPanel() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [intervalValue, setIntervalValue] = useState("5");
  const [isApplying, setIsApplying] = useState(false);

  const handleApply = async () => {
    const trimmed = intervalValue.trim();
    const parsed = Number(trimmed);

    if (
      !trimmed ||
      !Number.isInteger(parsed) ||
      parsed < 0 ||
      parsed > 86_400
    ) {
      toast.error(t("usage.routerTeamBatchInterval.invalidInterval"), {
        closeButton: true,
      });
      return;
    }

    setIsApplying(true);
    try {
      const updatedCount =
        await settingsApi.bulkUpdateRouterTeamUsageQueryInterval(parsed);

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["providers"] }),
        queryClient.invalidateQueries({ queryKey: usageKeys.all }),
      ]);

      toast.success(
        t("usage.routerTeamBatchInterval.updated", {
          count: updatedCount,
        }),
        { closeButton: true },
      );
    } catch (error) {
      toast.error(
        t("usage.routerTeamBatchInterval.updateFailed", {
          error: (error as Error)?.message ?? String(error),
        }),
        { closeButton: true },
      );
    } finally {
      setIsApplying(false);
    }
  };

  return (
    <div className="space-y-4">
      <div className="rounded-lg border border-emerald-500/15 bg-emerald-500/5 px-3 py-2">
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <Clock3 className="h-3.5 w-3.5 text-emerald-500" />
          <span>{t("usage.routerTeamBatchInterval.intervalHint")}</span>
        </div>
      </div>

      <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto] md:items-end">
        <div className="space-y-2">
          <label className="text-sm font-medium text-foreground/90">
            {t("usage.routerTeamBatchInterval.intervalLabel")}
          </label>
          <Input
            type="number"
            min={0}
            max={86400}
            step={1}
            value={intervalValue}
            onChange={(event) => setIntervalValue(event.target.value)}
            placeholder="5"
            className="h-10"
          />
        </div>

        <Button
          onClick={handleApply}
          disabled={isApplying}
          className="h-10 min-w-28"
        >
          {isApplying ? (
            <span className="inline-flex items-center gap-2">
              <Loader2 className="h-4 w-4 animate-spin" />
              {t("usage.routerTeamBatchInterval.applying")}
            </span>
          ) : (
            t("usage.routerTeamBatchInterval.apply")
          )}
        </Button>
      </div>
    </div>
  );
}
