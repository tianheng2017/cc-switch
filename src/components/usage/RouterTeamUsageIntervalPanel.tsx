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
      <div className="flex items-center gap-3">
        <Clock3 className="h-5 w-5 text-emerald-500" />
        <div className="text-left">
          <h3 className="text-base font-semibold">
            {t("usage.routerTeamBatchInterval.title")}
          </h3>
          <p className="text-sm text-muted-foreground">
            {t("usage.routerTeamBatchInterval.description")}
          </p>
        </div>
      </div>

      <div className="flex flex-col gap-3 sm:flex-row sm:items-end">
        <div className="flex-1 space-y-2">
          <label className="text-sm font-medium text-foreground">
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
          />
          <p className="text-xs text-muted-foreground">
            {t("usage.routerTeamBatchInterval.intervalHint")}
          </p>
        </div>

        <Button onClick={handleApply} disabled={isApplying}>
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
