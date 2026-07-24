import Fastify from "fastify";

const app = Fastify();
const port = Number(process.env.PORT || 8080);
const host = "0.0.0.0";

app.get("/", async () => {
  return "Hello from Fastify on Anybuild";
});

await app.listen({ host, port });
console.log(`Fastify server listening on http://${host}:${port}`);
