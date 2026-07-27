import { serve } from "@hono/node-server";
import { Hono } from "hono";

const app = new Hono();
const port = Number(process.env.PORT || 8080);
const host = "0.0.0.0";

app.get("/", (context) => {
  return context.text("Hello from Hono on Anybuild");
});

serve(
  {
    fetch: app.fetch,
    hostname: host,
    port,
  },
  (info) => {
    console.log(`Hono server listening on http://${host}:${info.port}`);
  },
);
