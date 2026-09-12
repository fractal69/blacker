<script setup lang="ts">
// BLACKER
// Copyright (C) 2026 Juan José Caballero Rey
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation version 3 of the License.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

import {
  computed,
  nextTick,
  onMounted,
  onUnmounted,
  ref,
  type ComponentPublicInstance,
} from "vue";
import { useBacktestingTabStore } from "~/stores/tabs";
import type { ChartEvent } from "~/packages/src/core/types";
import Chart, { type ChartTimeframe } from "~/components/Chart.vue";

const props = defineProps<{
  tabId: string;
}>();

const toast = useToast();
// -----------------------------------------------------------------------------
// Tab / Store
// -----------------------------------------------------------------------------

const tabManager = useTabManager();
const tab = tabManager.getTabById(props.tabId)!;
const tabStore = useBacktestingTabStore(tab as BacktestingTab);

// -----------------------------------------------------------------------------
// WebSocket
// -----------------------------------------------------------------------------
//
// Keep the session alive for the lifetime of this component.
//
const _session = useBacktestingSession(props.tabId, tabStore.symbol);

// -----------------------------------------------------------------------------
// State
// -----------------------------------------------------------------------------

const activeTimeframe = ref("1m");

const timeframeIds = computed(() =>
  Object.keys(tabStore.globalState.engine_state.timeframes),
);

// -----------------------------------------------------------------------------
// Chart.vue instances
// -----------------------------------------------------------------------------

type ChartInstance = InstanceType<typeof Chart>;

const charts = ref<Record<string, ChartInstance>>({});

/**
 * Registers or unregisters a Chart component instance for a specific timeframe.
 *
 * @param timeframeId - Unique identifier of the timeframe associated with the chart.
 * @param instance - Vue component instance, DOM element, or null when the component is unmounted.
 */
const setChartRef = (
  timeframeId: string,
  el: Element | ComponentPublicInstance | null,
) => {
  //
  // Vue passes null when the component is unmounted or the ref is removed.
  //
  if (!el) {
    delete charts.value[timeframeId];
    return;
  }
  //
  // Cast the Vue component instance to the exposed Chart component type.
  //
  const chart = el as ChartInstance;
  //
  // Vue can invoke the ref callback more than once.
  //
  if (charts.value[timeframeId] === chart) {
    return;
  }
  //
  // Register the Chart instance under its corresponding timeframe.
  //
  charts.value[timeframeId] = chart;
};

// -----------------------------------------------------------------------------
// Chart updates
// -----------------------------------------------------------------------------

/**
 * Series per timeframe that have already received their full initial history.
 *
 * Persisted per component instance: on reload the charts are rebuilt empty,
 * so the first update must fill all bars immediately. Only data arriving
 * after that is animated lazily.
 */
const loadedSeries = new Set<string>();

const loadedKey = (timeframeId: string, seriesId: string) =>
  `${timeframeId}/${seriesId}`;

/**
 * Rebuilds and updates a chart for a specific timeframe.
 */
const updateChart = async (timeframeId: string, timeframe: ChartTimeframe) => {
  //
  // Get the Chart component registered for this timeframe.
  //
  const chart = charts.value[timeframeId];
  if (!chart) return;
  //
  // Rebuild the chart structure and create all required series.
  //
  chart.applyLayout(timeframe);
  //
  // Wait for Vue and the chart DOM structure to finish updating.
  //
  await nextTick();
  //
  // Populate each series.
  //
  for (const [seriesId, series] of Object.entries(timeframe.series)) {
    const history = series?.history;

    if (!history?.length) continue;

    chart.applyOptions(seriesId, {
      legend: tabStore.globalState.symbol + " " + timeframe.id,
    });

    const key = loadedKey(timeframeId, seriesId);

    if (loadedSeries.has(key)) {
      //
      // Already loaded: animate only the newly arrived bars.
      //
      chart.patchDataLazy(seriesId, history);
    } else {
      //
      // First fill after reload: show the full history immediately.
      //
      chart.patchData(seriesId, history);

      loadedSeries.add(key);
    }
  }
};

/**
 * Updates all registered charts using the latest timeframe state.
 */
const updateCharts = async () => {
  //
  // Wait until Vue has completed the current rendering cycle.
  //
  await nextTick();
  //
  // Update each chart with its corresponding timeframe data.
  //
  const timeframes = tabStore.globalState.engine_state.timeframes;
  for (const [timeframeId, timeframe] of Object.entries(timeframes)) {
    await updateChart(timeframeId, timeframe as ChartTimeframe);
  }
};

/**
 * Handles chart events emitted by a Chart component.
 *
 * @param timeframeId - Identifier of the timeframe the chart belongs to.
 * @param event - Chart event dispatched by the underlying ChartEngine.
 */
const onChartEvent = async (timeframeId: string, event: ChartEvent) => {
  console.log(event);

  if (event.type === "series:removed") {
    try {
      await tabStore.deleteSeries(timeframeId, event.seriesId);
    } catch (err: any) {
      toast.add({
        title: "Error deleting series",
        description: err.data.message,
        icon: "i-lucide-circle-x",
        color: "error",
      });
    }
  }

  if (event.type === "series:params") {
    const series =
      tabStore.globalState.engine_state.timeframes[timeframeId]?.series[
        event.seriesId
      ];

    if (!series) {
      console.warn(`Cannot edit series "${event.seriesId}": not found.`);
      return;
    }

    try {
      await tabStore.editSeries(timeframeId, {
        ...series,
        params: event.params,
      });
    } catch (err: any) {
      toast.add({
        title: "Error editing series",
        description: err.data.message,
        icon: "i-lucide-circle-x",
        color: "error",
      });
    }
  }
};

// -----------------------------------------------------------------------------
// Store subscription
// -----------------------------------------------------------------------------

const unsubscribe = tabStore.listeners.subscribe(async (event) => {
  if (event.type !== "live-update") return;

  await updateCharts();
});

// -----------------------------------------------------------------------------
// Initial load
// -----------------------------------------------------------------------------

/**
 * On mount, paints the charts from the persisted state right away so a
 * reload shows the full history immediately. Subsequent engine messages
 * animate only the newly arrived bars.
 */
onMounted(async () => {
  await updateCharts();
});

// -----------------------------------------------------------------------------
// Cleanup
// -----------------------------------------------------------------------------

onUnmounted(() => {
  unsubscribe();
});
</script>

<template>
  <div class="backtesting-tab">
    <BacktestingToolbar
      :tab-id="tabId"
      :timeframes="timeframeIds"
      :active-timeframe="activeTimeframe"
      @update:timeframe="activeTimeframe = $event"
    />

    <div class="charts">
      <div
        v-for="timeframeId in timeframeIds"
        :key="timeframeId"
        class="chart-wrapper"
        :class="{
          'chart-wrapper--active': timeframeId === activeTimeframe,
        }"
      >
        <Chart
          :key="timeframeId"
          :ref="(el) => setChartRef(timeframeId, el)"
          :timeframeId="timeframeId"
          @chart="(event) => onChartEvent(timeframeId, event)"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.backtesting-tab {
  gap: 0.25rem;
  height: 100%;
  min-height: 0;
  display: flex;
  padding: var(--tab-content-padding);
  flex-direction: column;
  box-sizing: border-box;
}

.charts {
  position: relative;
  flex: 1;
  min-height: 0;
  width: 100%;
}

.chart-wrapper {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  visibility: hidden;
  pointer-events: none;
}

.chart-wrapper--active {
  visibility: visible;
  pointer-events: auto;
}
</style>
