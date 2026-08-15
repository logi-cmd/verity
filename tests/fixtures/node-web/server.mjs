import { createServer } from "node:http";
import { readFile } from "node:fs/promises";

const html = await readFile("dist/index.html", "utf8");
const server = createServer((request, response) => {
  if (request.url !== "/") { response.writeHead(404).end("not found"); return; }
  response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
  response.end(html);
});
server.listen(4173, "0.0.0.0");
