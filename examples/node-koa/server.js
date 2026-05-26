import Koa from 'koa';

const app = new Koa();
const port = process.env.PORT || 3000;

app.use((ctx) => {
  ctx.type = 'text/plain';
  ctx.body = 'Hello from Koa on Shipit';
});

app.listen(port, () => {
  console.log('Koa server listening on ' + port);
});
