self.onmessage = msg => (async () => {
    console.log("hello from worker")

    let { default: init, worker_main } = await import("./rv-web/pkg/rv_web.js");

    console.log("initializing module")
    await init({ module_or_path: "./rv-web/pkg/rv_web_bg.wasm" })

    console.log("running interpreter")
    let game = new Uint8Array(msg.data.game)
    await worker_main(game)
})()
    .catch(e => console.error("Worker internal error:", e.message))