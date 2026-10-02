import { expect, test, type Page } from "@playwright/test"

const SCREENS = [
  { route: "/", heading: "Dashboard" },
  { route: "/repositories", heading: "Repositories" },
  { route: "/repositories/r-file-processing", heading: "file_processing" },
  { route: "/repositories/r-file-processing/branches", heading: "file_processing" },
  { route: "/repositories/r-file-processing/history", heading: "file_processing" },
  { route: "/repositories/r-file-processing/logs", heading: "file_processing" },
  { route: "/sync", heading: "Sync Center" },
  { route: "/activity", heading: "Activity" },
  { route: "/connections", heading: "Connections" },
  { route: "/settings/general", heading: "Settings" },
  { route: "/settings/sync", heading: "Settings" },
]

const VIEWPORTS = [
  { width: 1440, height: 900 },
  { width: 1280, height: 800 },
  { width: 1024, height: 768 },
]

async function open(page: Page, route: string, scenario = "attention") {
  const errors: string[] = []
  page.on("pageerror", (error) => errors.push(String(error)))
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text())
  })
  await page.goto(`/demo.html?scenario=${scenario}#${route}`)
  return errors
}

for (const viewport of VIEWPORTS) {
  test.describe(`${viewport.width}x${viewport.height}`, () => {
    test.use({ viewport })
    for (const screen of SCREENS) {
      test(`${screen.route} renders without overflow or errors`, async ({ page }) => {
        const errors = await open(page, screen.route)
        await expect(page.getByRole("heading", { level: 1, name: screen.heading })).toBeVisible()
        const overflow = await page.evaluate(() => document.documentElement.scrollWidth > document.documentElement.clientWidth)
        expect(overflow).toBe(false)
        expect(errors).toEqual([])
      })
    }
  })
}

test("main navigation is keyboard reachable and marks the current page", async ({ page }) => {
  await open(page, "/")
  const nav = page.getByRole("navigation", { name: "Main" })
  for (const name of ["Repositories", "Sync Center", "Activity", "Connections", "Settings", "Dashboard"]) {
    const link = nav.getByRole("link", { name })
    await link.focus()
    await expect(link).toBeFocused()
    await page.keyboard.press("Enter")
    await expect(link).toHaveAttribute("aria-current", "page")
  }
})

test("unconfigured profile is routed to onboarding", async ({ page }) => {
  await open(page, "/", "onboarding")
  await expect(page).toHaveURL(/#\/onboarding/)
})

test("native failures are shown as a startup error, not a fake dashboard", async ({ page }) => {
  await open(page, "/", "unsupported")
  await expect(page.getByText("Git Backup Manager could not start")).toBeVisible()
  await expect(page.getByText("UNSUPPORTED", { exact: false }).first()).toBeVisible()
})

test("repository row actions are labeled icon buttons", async ({ page }) => {
  await open(page, "/repositories")
  await expect(page.getByRole("button", { name: /^Actions for / }).first()).toBeVisible()
})
