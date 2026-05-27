import { node } from "@elysia/node";
import { Elysia } from "elysia";

const port = Number(process.env.PORT || 8080);
const host = "0.0.0.0";

new Elysia({ adapter: node() })
  .get("/", () => "Hello from Elysia on Shipit")
  .listen(
    {
      hostname: host,
      port,
    },
    ({ port }) => {
      console.log(`Elysia server listening on http://${host}:${port}`);
    },
  );
