import { defineEventHandler, proxyRequest } from "h3";

export default defineEventHandler((event) => {
  const config = useRuntimeConfig(event);

  return proxyRequest(
    event,
    `http://localhost:3002/api/backtest/master/edit-series`
  );
});