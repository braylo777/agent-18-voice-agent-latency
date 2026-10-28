// Bundles src/core.js + src/index.js into dist/worker.js for API upload.
import fs from "node:fs"; const strip = (f) => fs.readFileSync(f, "utf8").replace(/^import .*\n/gm, "").replace(/^export (const|function|class)/gm, "$1");
fs.mkdirSync("dist", { recursive: true }); fs.writeFileSync("dist/worker.js", strip("src/core.js") + "\n" + strip("src/index.js").replace(/^const H/m, "const H"));
