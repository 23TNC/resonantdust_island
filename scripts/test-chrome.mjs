#!/usr/bin/env node
/**
 * Smoke test: load the page in Chrome on Windows and assert the hello world
 * actually works.
 *
 * Speaks the W3C WebDriver protocol to chromedriver over plain fetch, so there
 * is no npm dependency to install and nothing to keep in sync with Chrome.
 *
 * Chrome MUST be the Windows build. wgpu cannot reach the GPU from WSL, so a
 * Linux Chrome here would fall back to software and the test would pass while
 * proving nothing. That is also why a software adapter is a hard failure below
 * rather than a warning.
 *
 * Usage:
 *   node scripts/test-chrome.mjs [--headless] [--screenshot out.png]
 *
 * Env overrides: DRIVER_URL, PAGE_URL, CHROME_BINARY, USER_DATA_DIR
 */

import { writeFile } from 'node:fs/promises'

const DRIVER_URL = process.env.DRIVER_URL ?? 'http://localhost:9515'
const PAGE_URL = process.env.PAGE_URL ?? 'http://localhost:5173/'
const CHROME_BINARY =
  process.env.CHROME_BINARY ??
  'C:\\Program Files\\Google\\Chrome\\152.0.7977.42\\chrome-win64\\chrome.exe'
const USER_DATA_DIR = process.env.USER_DATA_DIR ?? 'C:\\temp\\rd-island-profile'

const args = process.argv.slice(2)
const HEADLESS = args.includes('--headless')
const screenshotIdx = args.indexOf('--screenshot')
const SCREENSHOT = screenshotIdx >= 0 ? args[screenshotIdx + 1] : null

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

async function wd(method, path, body) {
  const res = await fetch(`${DRIVER_URL}${path}`, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  })
  const text = await res.text()
  let json
  try {
    json = JSON.parse(text)
  } catch {
    throw new Error(`${method} ${path}: non-JSON response (${res.status}): ${text.slice(0, 300)}`)
  }
  if (!res.ok || json.value?.error) {
    const v = json.value ?? {}
    throw new Error(`${method} ${path}: ${v.error ?? res.status} — ${v.message ?? text.slice(0, 300)}`)
  }
  return json.value
}

const chromeArgs = [
  `--user-data-dir=${USER_DATA_DIR}`,
  // A GPU process is the entire point; never let it fall back quietly.
  '--enable-features=Vulkan',
]
// Deliberately NOT passed: --enable-unsafe-swiftshader. It would make a
// headless run go green on a software rasteriser, which is precisely the
// failure this test exists to catch. See issues.md §6.
if (HEADLESS) chromeArgs.push('--headless=new', '--window-size=1280,900')

let sessionId = null
const failures = []
const notes = []

function check(ok, message) {
  if (ok) notes.push(`  PASS  ${message}`)
  else failures.push(message)
}

try {
  console.log(`==> chromedriver at ${DRIVER_URL}`)
  const status = await wd('GET', '/status')
  console.log(`    ${status.message ?? 'ready'}`)

  console.log(`==> launching ${HEADLESS ? 'headless' : 'headed'} Chrome`)
  const session = await wd('POST', '/session', {
    capabilities: {
      alwaysMatch: {
        browserName: 'chrome',
        'goog:chromeOptions': { binary: CHROME_BINARY, args: chromeArgs },
        'goog:loggingPrefs': { browser: 'ALL' },
      },
    },
  })
  sessionId = session.sessionId
  const s = `/session/${sessionId}`

  console.log(`==> navigating to ${PAGE_URL}`)
  await wd('POST', `${s}/url`, { url: PAGE_URL })

  // Poll until the shell reports a terminal state. Adapter acquisition and
  // shader compilation are not instant, especially on a cold shader cache.
  const DEADLINE_MS = 30_000
  const started = Date.now()
  let state = null
  let statusText = ''
  while (Date.now() - started < DEADLINE_MS) {
    const result = await wd('POST', `${s}/execute/sync`, {
      script: `const el = document.getElementById('status');
               return { state: el?.dataset.state ?? null, text: el?.textContent ?? '' };`,
      args: [],
    })
    state = result.state
    statusText = result.text
    if (state) break
    await sleep(250)
  }

  const elapsed = ((Date.now() - started) / 1000).toFixed(1)
  console.log(`==> status after ${elapsed}s: ${state ?? 'STILL BOOTING'}`)
  console.log(
    statusText
      .split('\n')
      .map((l) => `    ${l}`)
      .join('\n'),
  )

  check(state !== null, 'the page reached a terminal state (did not hang booting)')
  check(state !== 'error', 'the worker booted without error')
  check(state !== 'software', 'the adapter is not a software rasteriser')

  const vendor = await wd('POST', `${s}/execute/sync`, {
    script: 'return document.documentElement.dataset.vendor ?? "";',
    args: [],
  })
  console.log(`==> adapter vendor: ${vendor || '(withheld by the browser)'}`)

  // 'unknown' means not-a-fallback-adapter but with no positive hardware
  // evidence. Reported, not failed — the browser is entitled to withhold the
  // vendor, and failing would make the test depend on a privacy setting.
  if (state === 'unknown') {
    notes.push('  WARN  adapter vendor withheld; cannot positively confirm hardware')
  }
  check(state === 'ok' || state === 'unknown', 'status is ok (or unconfirmed), not an error')

  // Prove the loop is advancing rather than a single frame having been drawn.
  // The count comes from the renderer and counts presented frames.
  const readFrames = () =>
    wd('POST', `${s}/execute/sync`, {
      script: 'return Number(document.documentElement.dataset.frames ?? 0);',
      args: [],
    })

  // Wait for the first stats sample before measuring. The worker posts the
  // count every 30 ticks, so reading immediately after 'ready' reliably sees
  // zero — which says nothing about whether frames are being drawn.
  let first = 0
  const frameDeadline = Date.now() + 5_000
  while (Date.now() < frameDeadline) {
    first = await readFrames()
    if (first > 0) break
    await sleep(100)
  }
  check(first > 0, 'at least one frame was presented')

  await sleep(1500)
  const second = await readFrames()
  const fps = ((second - first) / 1.5).toFixed(0)
  console.log(`==> frames presented: ${first} -> ${second} over 1.5s (~${fps}/s)`)
  check(second > first, 'the frame count advanced (render loop is running)')

  if (SCREENSHOT) {
    const b64 = await wd('GET', `${s}/screenshot`)
    await writeFile(SCREENSHOT, Buffer.from(b64, 'base64'))
    console.log(`==> screenshot written to ${SCREENSHOT}`)
  }

  // Browser console. Anything at SEVERE is a failure; the page is expected to
  // be quiet apart from our own info logs.
  let logs = []
  try {
    logs = await wd('POST', `${s}/log`, { type: 'browser' })
  } catch {
    notes.push('  SKIP  browser log unavailable from this driver')
  }
  const severe = logs.filter((l) => l.level === 'SEVERE')
  if (logs.length) {
    console.log('==> browser console:')
    for (const l of logs) console.log(`    [${l.level}] ${l.message.slice(0, 200)}`)
  }
  check(severe.length === 0, 'no SEVERE console messages')
} catch (err) {
  failures.push(`threw: ${err.message}`)
} finally {
  if (sessionId) {
    try {
      await wd('DELETE', `/session/${sessionId}`)
    } catch {
      /* session already gone */
    }
  }
}

console.log('')
for (const n of notes) console.log(n)
for (const f of failures) console.log(`  FAIL  ${f}`)
console.log('')

if (failures.length) {
  console.log(`FAILED — ${failures.length} check(s)`)
  process.exit(1)
}
console.log('PASSED')
