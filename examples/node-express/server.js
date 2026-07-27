import express from 'express';

const app = express();
const port = process.env.PORT || 3000;

app.get('/', (_request, response) => {
  response.type('text/plain').send('Hello from Express on Anybuild');
});

app.listen(port, () => {
  console.log('Express server listening on ' + port);
});
