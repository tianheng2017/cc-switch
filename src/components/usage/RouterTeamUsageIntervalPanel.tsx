import { useEffect, useMemo, useState } from "react";
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
  const [intervalValue, setIntervalValue] = useState("");
  const [loadedIntervalValue, setLoadedIntervalValue] = useState("");
  const [passwordValue, setPasswordValue] = useState("");
  const [loadedPasswordValue, setLoadedPasswordValue] = useState("");
  const [thresholdValue, setThresholdValue] = useState("0.1");
  const [loadedThresholdValue, setLoadedThresholdValue] = useState("0.1");
  const [isMixedInterval, setIsMixedInterval] = useState(false);
  const [isLoadingSettings, setIsLoadingSettings] = useState(true);
  const [isSavingInterval, setIsSavingInterval] = useState(false);
  const [isSavingPassword, setIsSavingPassword] = useState(false);
  const [isSavingThreshold, setIsSavingThreshold] = useState(false);

  useEffect(() => {
    let active = true;

    const loadSettings = async () => {
      try {
        const settings = await settingsApi.getRouterTeamUsageBatchSettings();
        if (!active) {
          return;
        }

        const nextIntervalValue =
          settings.intervalSecs === null ? "" : String(settings.intervalSecs);

        setIntervalValue(nextIntervalValue);
        setLoadedIntervalValue(nextIntervalValue);
        setPasswordValue(settings.password);
        setLoadedPasswordValue(settings.password);
        const nextThresholdValue = String(settings.degradedThreshold);
        setThresholdValue(nextThresholdValue);
        setLoadedThresholdValue(nextThresholdValue);
        setIsMixedInterval(settings.mixedInterval);
      } catch (error) {
        if (!active) {
          return;
        }
        toast.error(
          t("usage.routerTeamBatchInterval.loadFailed", {
            error: (error as Error)?.message ?? String(error),
          }),
          { closeButton: true },
        );
      } finally {
        if (active) {
          setIsLoadingSettings(false);
        }
      }
    };

    void loadSettings();

    return () => {
      active = false;
    };
  }, [t]);

  const intervalDirty = useMemo(() => {
    return isMixedInterval || intervalValue !== loadedIntervalValue;
  }, [intervalValue, isMixedInterval, loadedIntervalValue]);

  const passwordDirty = passwordValue !== loadedPasswordValue;
  const thresholdDirty = thresholdValue !== loadedThresholdValue;

  const handleSaveInterval = async () => {
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

    setIsSavingInterval(true);
    try {
      const updatedCount =
        await settingsApi.bulkUpdateRouterTeamUsageQueryInterval(parsed);

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["providers"] }),
        queryClient.invalidateQueries({ queryKey: usageKeys.all }),
      ]);

      const normalizedValue = String(parsed);
      setIntervalValue(normalizedValue);
      setLoadedIntervalValue(normalizedValue);
      setIsMixedInterval(false);

      toast.success(
        t("usage.routerTeamBatchInterval.intervalUpdated", {
          count: updatedCount,
        }),
        { closeButton: true },
      );
    } catch (error) {
      toast.error(
        t("usage.routerTeamBatchInterval.intervalUpdateFailed", {
          error: (error as Error)?.message ?? String(error),
        }),
        { closeButton: true },
      );
    } finally {
      setIsSavingInterval(false);
    }
  };

  const handleSavePassword = async () => {
    setIsSavingPassword(true);
    try {
      await settingsApi.saveRouterTeamUsageLoginPassword(passwordValue);
      setLoadedPasswordValue(passwordValue);

      toast.success(t("usage.routerTeamBatchInterval.passwordUpdated"), {
        closeButton: true,
      });
    } catch (error) {
      toast.error(
        t("usage.routerTeamBatchInterval.passwordUpdateFailed", {
          error: (error as Error)?.message ?? String(error),
        }),
        { closeButton: true },
      );
    } finally {
      setIsSavingPassword(false);
    }
  };

  const handleSaveThreshold = async () => {
    const trimmed = thresholdValue.trim();
    const parsed = Number(trimmed);

    if (!trimmed || !Number.isFinite(parsed) || parsed < 0) {
      toast.error(t("usage.routerTeamBatchInterval.invalidThreshold"), {
        closeButton: true,
      });
      return;
    }

    setIsSavingThreshold(true);
    try {
      const updatedCount =
        await settingsApi.saveRouterTeamUsageDegradedThreshold(parsed);

      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["providers"] }),
        queryClient.invalidateQueries({ queryKey: usageKeys.all }),
      ]);

      const normalizedValue = String(parsed);
      setThresholdValue(normalizedValue);
      setLoadedThresholdValue(normalizedValue);

      toast.success(
        t("usage.routerTeamBatchInterval.thresholdUpdated", {
          count: updatedCount,
        }),
        { closeButton: true },
      );
    } catch (error) {
      toast.error(
        t("usage.routerTeamBatchInterval.thresholdUpdateFailed", {
          error: (error as Error)?.message ?? String(error),
        }),
        { closeButton: true },
      );
    } finally {
      setIsSavingThreshold(false);
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

      <div className="rounded-lg border border-border/60 bg-card/40 p-4">
        <div className="space-y-3">
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
              onChange={(event) => {
                setIntervalValue(event.target.value);
                if (isMixedInterval) {
                  setIsMixedInterval(false);
                }
              }}
              placeholder={
                isMixedInterval
                  ? t("usage.routerTeamBatchInterval.mixedPlaceholder")
                  : "5"
              }
              className="h-10"
              disabled={isSavingInterval || isLoadingSettings}
            />
          </div>

          <p className="text-xs leading-5 text-muted-foreground">
            {t("usage.routerTeamBatchInterval.description")}
          </p>

          {isMixedInterval && (
            <p className="text-xs leading-5 text-amber-600 dark:text-amber-400">
              {t("usage.routerTeamBatchInterval.mixedHint")}
            </p>
          )}

          <div className="flex justify-end pt-1">
            <Button
              onClick={handleSaveInterval}
              disabled={isSavingInterval || isLoadingSettings || !intervalDirty}
              className="h-10 min-w-32"
            >
              {isSavingInterval ? (
                <span className="inline-flex items-center gap-2">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t("usage.routerTeamBatchInterval.intervalApplying")}
                </span>
              ) : (
                t("usage.routerTeamBatchInterval.intervalApply")
              )}
            </Button>
          </div>
        </div>

        <div className="mt-5 space-y-3 border-t border-border/50 pt-5">
          <div className="space-y-2">
            <label className="text-sm font-medium text-foreground/90">
              {t("usage.routerTeamBatchInterval.passwordLabel")}
            </label>
            <Input
              type="password"
              value={passwordValue}
              onChange={(event) => setPasswordValue(event.target.value)}
              placeholder={t("usage.routerTeamBatchInterval.passwordPlaceholder")}
              className="h-10"
              autoComplete="off"
              disabled={isSavingPassword || isLoadingSettings}
            />
          </div>

          <p className="text-xs leading-5 text-muted-foreground">
            {t("usage.routerTeamBatchInterval.passwordHint")}
          </p>

          <div className="flex justify-end pt-1">
            <Button
              onClick={handleSavePassword}
              disabled={isSavingPassword || isLoadingSettings || !passwordDirty}
              className="h-10 min-w-28"
            >
              {isSavingPassword ? (
                <span className="inline-flex items-center gap-2">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t("usage.routerTeamBatchInterval.passwordSaving")}
                </span>
              ) : (
                t("usage.routerTeamBatchInterval.passwordApply")
              )}
            </Button>
          </div>
        </div>

        <div className="mt-5 space-y-3 border-t border-border/50 pt-5">
          <div className="space-y-2">
            <label className="text-sm font-medium text-foreground/90">
              {t("usage.routerTeamBatchInterval.thresholdLabel")}
            </label>
            <Input
              type="number"
              min={0}
              step="0.01"
              value={thresholdValue}
              onChange={(event) => setThresholdValue(event.target.value)}
              placeholder="0.1"
              className="h-10"
              disabled={isSavingThreshold || isLoadingSettings}
            />
          </div>

          <p className="text-xs leading-5 text-muted-foreground">
            {t("usage.routerTeamBatchInterval.thresholdHint")}
          </p>

          <div className="flex justify-end pt-1">
            <Button
              onClick={handleSaveThreshold}
              disabled={isSavingThreshold || isLoadingSettings || !thresholdDirty}
              className="h-10 min-w-32"
            >
              {isSavingThreshold ? (
                <span className="inline-flex items-center gap-2">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  {t("usage.routerTeamBatchInterval.thresholdSaving")}
                </span>
              ) : (
                t("usage.routerTeamBatchInterval.thresholdApply")
              )}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
