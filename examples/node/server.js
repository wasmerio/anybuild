const http = require("node:http");

const port = Number(process.env.PORT || 8080);
const host = "0.0.0.0";

const server = http.createServer((request, response) => {
  response.writeHead(200, { "Content-Type": "text/plain; charset=utf-8" });
  response.end("Hello from Node");
});

server.listen(port, host, () => {
  console.log(`Node server listening on http://${host}:${port}`);
});
