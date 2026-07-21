import { pickDirectory, restoreDirectory, requestPermission, collectKmzFiles } from "../shared/fs.js";
import { buildLocationEntries, loadPlacemarks, dateKeyFromFilename } from "../shared/grouping.js";

const connectRow = document.getElementById("connectRow");
const pilotListEl = document.getElementById("pilotList");
const logbookView = document.getElementById("logbookView");
const pilotNameEl = document.getElementById("pilotName");
const pilotTotalsEl = document.getElementById("pilotTotals");
const typeBreakdownEl = document.getElementById("typeBreakdown");
const flightRowsEl = document.getElementById("flightRows");
const backBtn = document.getElementById("backBtn");

/** "1h 29m 30s" / "29m 30s" (core::kml::format_duration's exact output shapes) -> seconds. */
function parseDurationToSeconds(text) {
  const m = /^(?:(\d+)h\s+)?(\d+)m\s+(\d+)s$/.exec((text ?? "").trim());
  if (!m) return 0;
  const [, h, min, s] = m;
  return (Number(h) || 0) * 3600 + Number(min) * 60 + Number(s);
}

function formatHours(totalSeconds) {
  const totalMinutes = Math.round(totalSeconds / 60);
  const h = Math.floor(totalMinutes / 60);
  const m = totalMinutes % 60;
  return `${h}h ${m}m`;
}

/** "20260615" -> "06/15/2026", or "Unknown date" if unparseable. */
function formatDateKey(dateKey) {
  if (!dateKey) return "Unknown date";
  return `${dateKey.slice(4, 6)}/${dateKey.slice(6, 8)}/${dateKey.slice(0, 4)}`;
}

/**
 * Loads every location's placemarks and flattens them into logbook rows.
 * Unlike the viewer, coordinates are discarded immediately — the logbook
 * never draws anything, no reason to hold GPS point arrays in memory for
 * what could be a large dataset.
 */
async function buildFlightRows(locations) {
  const rows = [];
  for (const entry of locations) {
    let placemarks;
    try {
      placemarks = await loadPlacemarks(entry);
    } catch (err) {
      console.warn(`Failed to load ${entry.folderName}:`, err);
      continue;
    }
    for (const pm of placemarks) {
      rows.push({
        pilot: pm.meta.pilot || "Unknown Pilot",
        // The local date embedded in the flight's own filename (the
        // placemark's name), not the UTC "Start Time" in the
        // description — those can land on different calendar days
        // depending on timezone, and a logbook date must be the local
        // one the pilot actually flew.
        dateKey: dateKeyFromFilename(pm.name),
        aircraft: pm.meta.droneModel || "Unknown",
        location: entry.folderName,
        durationSeconds: parseDurationToSeconds(pm.meta.duration),
      });
    }
  }
  return rows;
}

function groupByPilot(rows) {
  const byPilot = new Map();
  for (const row of rows) {
    if (!byPilot.has(row.pilot)) byPilot.set(row.pilot, []);
    byPilot.get(row.pilot).push(row);
  }
  return byPilot;
}

function renderPilotList(byPilot) {
  pilotListEl.innerHTML = "";
  const pilots = Array.from(byPilot.keys()).sort((a, b) => a.localeCompare(b));

  if (pilots.length === 0) {
    pilotListEl.textContent = "No flights found in this folder.";
    return;
  }

  for (const pilot of pilots) {
    const flights = byPilot.get(pilot);
    const totalSeconds = flights.reduce((sum, f) => sum + f.durationSeconds, 0);

    const row = document.createElement("div");
    row.className = "pilot-row";

    const btn = document.createElement("button");
    btn.textContent = pilot;
    btn.addEventListener("click", () => showPilot(pilot, flights));

    const summary = document.createElement("span");
    summary.className = "pilot-summary";
    summary.textContent = `${formatHours(totalSeconds)} — ${flights.length} flight${flights.length === 1 ? "" : "s"}`;

    row.appendChild(btn);
    row.appendChild(summary);
    pilotListEl.appendChild(row);
  }
}

function showPilot(pilot, flights) {
  pilotNameEl.textContent = pilot;

  const totalSeconds = flights.reduce((sum, f) => sum + f.durationSeconds, 0);
  pilotTotalsEl.textContent = `Total: ${formatHours(totalSeconds)} across ${flights.length} flight${flights.length === 1 ? "" : "s"}`;

  const byAircraft = new Map();
  for (const f of flights) {
    byAircraft.set(f.aircraft, (byAircraft.get(f.aircraft) ?? 0) + f.durationSeconds);
  }
  typeBreakdownEl.textContent = Array.from(byAircraft.entries())
    .sort((a, b) => b[1] - a[1])
    .map(([aircraft, secs]) => `${aircraft}: ${formatHours(secs)}`)
    .join(" · ");

  flightRowsEl.innerHTML = "";
  const sorted = [...flights].sort((a, b) => (b.dateKey ?? "").localeCompare(a.dateKey ?? ""));
  for (const f of sorted) {
    const tr = document.createElement("tr");
    for (const value of [formatDateKey(f.dateKey), f.aircraft, f.location, formatHours(f.durationSeconds)]) {
      const td = document.createElement("td");
      td.textContent = value;
      tr.appendChild(td);
    }
    flightRowsEl.appendChild(tr);
  }

  pilotListEl.style.display = "none";
  logbookView.style.display = "block";
}

backBtn.addEventListener("click", () => {
  logbookView.style.display = "none";
  pilotListEl.style.display = "";
});

async function loadFromHandle(handle) {
  connectRow.innerHTML = `Connected: <strong>${handle.name}</strong>`;
  pilotListEl.textContent = "Scanning folder…";
  const entries = await collectKmzFiles(handle);
  const locations = buildLocationEntries(entries);
  if (locations.length === 0) {
    pilotListEl.textContent = "No .kmz files found in this folder.";
    return;
  }
  pilotListEl.textContent = "Loading flights…";
  const rows = await buildFlightRows(locations);
  renderPilotList(groupByPilot(rows));
}

function renderChooseButton() {
  connectRow.innerHTML = "";
  const btn = document.createElement("button");
  btn.textContent = "Choose Folder";
  btn.addEventListener("click", async () => {
    try {
      const handle = await pickDirectory();
      await loadFromHandle(handle);
    } catch (err) {
      if (err.name !== "AbortError") {
        connectRow.textContent = `Error: ${err.message ?? err}`;
      }
    }
  });
  connectRow.appendChild(btn);
}

function renderReconnectButton(handle) {
  connectRow.innerHTML = "";
  const btn = document.createElement("button");
  btn.textContent = `Reconnect to "${handle.name}"`;
  btn.addEventListener("click", async () => {
    const granted = await requestPermission(handle);
    if (granted) {
      await loadFromHandle(handle);
    } else {
      connectRow.textContent = "Permission denied.";
    }
  });
  connectRow.appendChild(btn);
}

async function init() {
  if (!window.showDirectoryPicker) {
    connectRow.textContent = "This browser doesn't support folder access — please use Chrome or Edge.";
    return;
  }

  const restored = await restoreDirectory();
  if (!restored) {
    renderChooseButton();
    return;
  }
  if (!restored.granted) {
    renderReconnectButton(restored.handle);
    return;
  }
  await loadFromHandle(restored.handle);
}

init();
