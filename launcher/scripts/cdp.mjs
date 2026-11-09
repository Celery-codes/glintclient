// Development tool: drive the running launcher's webview over the Chrome DevTools
// Protocol, to script the real UI and take screenshots. Start the launcher with
//   WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333 npm run tauri dev
// then:
//   node scripts/cdp.mjs eval "<js expression>"   -> prints the JSON result
//   node scripts/cdp.mjs shot <file.png>          -> saves a screenshot
//   node scripts/cdp.mjs wait <ms>
// Commands can be chained: node scripts/cdp.mjs eval "..." wait 500 shot a.png
const PORT = process.env.CDP_PORT ?? "9333";

const targets = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
const page = targets.find((t) => t.type === "page");
if (!page) throw new Error("no page target");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  ws.onopen = resolve;
  ws.onerror = reject;
});

let nextId = 1;
const pending = new Map();
ws.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id && pending.has(message.id)) {
    pending.get(message.id)(message);
    pending.delete(message.id);
  }
};
const send = (method, params = {}) =>
  new Promise((resolve) => {
    const id = nextId++;
    pending.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params }));
  });

const args = process.argv.slice(2);
for (let i = 0; i < args.length; i += 2) {
  const [command, value] = [args[i], args[i + 1]];
  if (command === "eval") {
    const reply = await send("Runtime.evaluate", {
      expression: value,
      awaitPromise: true,
      returnByValue: true,
    });
    const result = reply.result;
    if (result.exceptionDetails) {
      console.log("EXCEPTION:", result.exceptionDetails.exception?.description ?? result.exceptionDetails.text);
    } else {
      console.log(JSON.stringify(result.result.value, null, 1));
    }
  } else if (command === "shot") {
    const reply = await send("Page.captureScreenshot", { format: "png" });
    const { writeFileSync } = await import("node:fs");
    writeFileSync(value, Buffer.from(reply.result.data, "base64"));
    console.log("saved", value);
  } else if (command === "wait") {
    await new Promise((resolve) => setTimeout(resolve, Number(value)));
  }
}
ws.close();
