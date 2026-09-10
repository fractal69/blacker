import type { ChartTimeframe, Series } from "~/components/Chart.vue";
/**
 * EngineState def.
 */
export interface EngineState {
  tick_index: number;
  time: number;
  timeframes: Record<string, ChartTimeframe>;
  strategy: EngineStrategy;
}

type Value = unknown;

/**
 * EngineStrategy.
 */
export interface EngineStrategy {
  kind: string;
  params: Record<string, Value>;
}

/**
 * BacktestSessionMessage.
 */
export interface MasterState {
  symbol: string;
  config_hash: string;
  status: string;
  replay_status: string;
  replay_step: string;
  tick_index: number;
  engine_state: EngineState;
}

/**
 * Store event
 */
type BacktestingTabStoreEvent = { type: "live-update"; data: any };

/**
 * Store event handler.
 */
type Listener = (event: BacktestingTabStoreEvent) => void;

/**
 * Store definition.
 */
export const useBacktestingTabStore = (tab: BacktestingTab) =>
  defineStore(
    `tab/${tab.id}`,
    () => {
      //---------------------------------------------------------------------
      // STORE
      //---------------------------------------------------------------------

      const globalState = ref<MasterState>({
        symbol: "",
        config_hash: "",
        status: "",
        replay_status: "",
        replay_step: "",
        tick_index: 0,
        engine_state: {
          tick_index: 0,
          time: 0,
          timeframes: {},
          strategy: { kind: "", params: {} },
        },
      });

      const id = tab.id;
      const symbol = ref(tab.symbol);
      const tabTitle = computed(() => `${symbol.value} BT`);
      const tabColor = "warning";

      const status = computed(() => globalState.value.status);
      const replayStatus = computed(() => globalState.value.replay_status);

      const isReady = computed(() => status.value === "Ready");
      const isPending = computed(() => status.value === "Pending");

      const isRunning = computed(() => replayStatus.value === "Running");
      const isStopped = computed(() => replayStatus.value === "Stopped");

      const isPlayable = computed(
        () => Object.keys(globalState.value.engine_state.timeframes).length > 0,
      );

      const isEngineConnected = computed(() => true);

      const isExecutionConnected = computed(() => true);

      //---------------------------------------------------------------------
      // SUBSCRIPTIONS
      //---------------------------------------------------------------------

      const listeners = (() => {
        const set = new Set<Listener>();

        return {
          subscribe(listener: Listener) {
            set.add(listener);
            return () => set.delete(listener);
          },

          notify(event: BacktestingTabStoreEvent) {
            set.forEach((listener) => listener(event));
          },
        };
      })();

      //---------------------------------------------------------------------
      // METHODS
      //---------------------------------------------------------------------

      /**
       * Update session ws data.
       */
      function updateSession(data: MasterState) {
        globalState.value = data;

        listeners.notify({
          type: "live-update",
          data,
        });
      }

      /**
       * Start backtest.
       */
      async function startBacktest() {
        try {
          return await $fetch("/api/backtest/master/start-backtest", {
            method: "POST",
            body: {},
          });
        } catch (err: any) {
          console.error("[BacktestingTabStore] Failed to start backtest:", err);
          throw err;
        }
      }

      /**
       * Stop backtest.
       */
      async function stopBacktest() {
        try {
          return await $fetch("/api/backtest/master/stop-backtest", {
            method: "POST",
            body: {},
          });
        } catch (err: any) {
          console.error("[BacktestingTabStore] Failed to stop backtest:", err);
          throw err;
        }
      }

      /**
       * Add timeframe request.
       */
      async function addTimeframe(timeframeId: string) {
        try {
          return await $fetch("/api/backtest/master/add-timeframe", {
            method: "POST",
            body: {
              id: timeframeId,
            },
          });
        } catch (err: any) {
          console.error("[BacktestingTabStore] Failed to add timeframe:", err);
          throw err;
        }
      }

      /**
       * Add series request.
       */
      async function addSeries(timeframeId: string, series: Series) {
        try {
          return await $fetch("/api/backtest/master/add-series", {
            method: "POST",
            body: {
              timeframe_id: timeframeId,
              ...series,
            },
          });
        } catch (err: any) {
          console.error("[BacktestingTabStore] Failed to add series:", err);
          throw err;
        }
      }

      /**
       * Edit series request.
       */
      async function editSeries(timeframeId: string, series: Series) {
        try {
          return await $fetch("/api/backtest/master/edit-series", {
            method: "POST",
            body: {
              timeframe_id: timeframeId,
              id: series.id,
              overlay: series.overlay,
              params: series.params
            },
          });
        } catch (err: any) {
          console.error("[BacktestingTabStore] Failed to edit series:", err);
          throw err;
        }
      }

      /**
       * Returns the associated tab instance.
       */
      function getTab() {
        return tab;
      }

      /**
       * Lifecycle hook executed when the tab is mounted.
       */
      function onMount() {}

      /**
       * Lifecycle hook executed when the tab is unmounted.
       * Ensures the session is paused and disconnected.
       */
      function onUnmount() {}

      return {
        editSeries,
        addSeries,
        addTimeframe,
        isExecutionConnected,
        isEngineConnected,
        isPending,
        stopBacktest,
        isRunning,
        startBacktest,
        listeners,
        updateSession,
        tabTitle,
        tabColor,
        symbol,
        id,
        getTab,
        isPlayable,
        globalState,
        isReady,
        isStopped,
        onMount,
        onUnmount,
      };
    },
    {
      persist: {
        key: `backtesting-tab-${tab.id}`,

        serializer: {
          serialize: (state) => {
            return JSON.stringify({
              symbol: state.symbol,
              globalState: state.globalState,
            });
          },

          deserialize: (raw) => {
            try {
              const parsed = JSON.parse(raw);

              return {
                symbol: parsed.symbol,
                globalState: parsed.globalState,
              };
            } catch (err) {
              console.warn(
                `[backtesting-tab-${tab.id}] Estado corrupto, reseteando:`,
                err,
              );

              return {
                symbol: "BTCUSDT",
                globalState: {},
              };
            }
          },
        },
      },
    },
  )();
