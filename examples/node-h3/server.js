import { createServer } from 'node:http';
import { createApp, eventHandler, toNodeListener } from 'h3';

const app = createApp();
const port = process.env.PORT || 3000;

app.use('/', eventHandler(() => 'Hello from H3 on Shipit'));

createServer(toNodeListener(app)).listen(port, () => {
  console.log('H3 server listening on ' + port);
});
