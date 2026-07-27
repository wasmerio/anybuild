import { createServer } from 'node:http';

const port = process.env.PORT || 3000;
const body = 'Hello from XMCP on Anybuild';

createServer((request, response) => {
  response.writeHead(200, { 'content-type': 'text/plain; charset=utf-8' });
  response.end(body);
}).listen(port, () => {
  console.log('XMCP server listening on ' + port);
});
