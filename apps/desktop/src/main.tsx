import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import "./index.css"
import { App } from "./app/index.ts"
import { callNative } from "./lib/native/index.ts"

const rootElement = document.getElementById("root")

if (rootElement === null) {
  throw new Error("Missing #root element in index.html")
}

createRoot(rootElement).render(
  <StrictMode>
    <App port={{ kind: "native", call: callNative }} />
  </StrictMode>
)
