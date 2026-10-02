// TEST/DEMO ENTRY (demo.html). Renders the production App with the labeled fixture
// port. Not part of the production bundle: vite build uses index.html only.

import { StrictMode } from "react"
import { createRoot } from "react-dom/client"

import "@/index.css"
import { App } from "@/app"

import type { Scenario } from "./data.ts"
import { createFixturePort } from "./fixtureAdapter.ts"

const SCENARIOS: Scenario[] = ["attention", "healthy", "empty", "onboarding", "unsupported"]
const requested = new URLSearchParams(window.location.search).get("scenario")
const scenario = SCENARIOS.find((item) => item === requested) ?? "attention"

const rootElement = document.getElementById("root")
if (rootElement === null) throw new Error("Missing #root element in demo.html")

createRoot(rootElement).render(
  <StrictMode>
    <App port={createFixturePort(scenario)} />
  </StrictMode>,
)
