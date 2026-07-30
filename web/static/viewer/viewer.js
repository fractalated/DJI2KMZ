import { pickDirectory, restoreDirectory, requestPermission, collectKmzFiles } from "../shared/fs.js";
import { buildProjectTree, formatDateKey, dateKeyFromFilename, loadLabeledPlacemarks } from "../shared/grouping.js";
import { initMap, setFlightLayer, removeFlightLayer, fitToCoordinates } from "./map.js";

const connectRow = document.getElementById("connectRow");
const locationList = document.getElementById("locationList");
const clearAllBtn = document.getElementById("clearAllBtn");
const map = initMap("map");

// A few distinguishable line colors, cycled per visible flight so
// multiple simultaneously-checked flights (within or across projects)
// stay tellable apart on the map.
const PALETTE = ["#0080ff", "#ff6b00", "#00c853", "#e91e63", "#9c27b0", "#ffc107"];
let colorIndex = 0;
const flightColors = new Map(); // layerId -> color, stable for the life of the page

function colorFor(layerId) {
  if (!flightColors.has(layerId)) {
    flightColors.set(layerId, PALETTE[colorIndex % PALETTE.length]);
    colorIndex++;
  }
  return flightColors.get(layerId);
}

function visibleLayerIds() {
  return Array.from(document.querySelectorAll(".flight-checklist input:checked")).map((el) => el.dataset.layerId);
}

function refitToVisible() {
  const ids = visibleLayerIds();
  const coords = ids.map((id) => JSON.parse(document.querySelector(`[data-layer-id="${CSS.escape(id)}"]`).dataset.coords));
  if (coords.length > 0) fitToCoordinates(map, coords);
}

/**
 * Flattens every one of `entries`' flights (a project spans multiple
 * date-folder entries; a standalone/ungrouped location is just a
 * one-element array) into a single sorted list, each tagged with a
 * globally-unique layerId and a date-bearing label. One entry failing to
 * load doesn't block the rest.
 */
async function loadFlightsForEntries(entries) {
  const flights = [];
  for (const entry of entries) {
    let labeled;
    try {
      labeled = await loadLabeledPlacemarks(entry);
    } catch (err) {
      console.warn(`Failed to load ${entry.folderKey}:`, err);
      labeled = [];
    }
    for (const { pm, label } of labeled) {
      flights.push({
        layerId: `${entry.folderKey}::${pm.name}`,
        label,
        coordinates: pm.coordinates,
        duration: pm.meta.duration,
        distance: pm.meta.distance,
      });
    }
  }
  flights.sort((a, b) => (dateKeyFromFilename(b.label) ?? "").localeCompare(dateKeyFromFilename(a.label) ?? ""));
  return flights;
}

/**
 * Renders `flights` as a checklist inside `container` — every flight
 * checked (and drawn on the map) by default, since the point of clicking
 * a project is to see everything at once, not build up a selection one
 * checkbox at a time. A Select all/Deselect all pair sits above the list
 * for quickly narrowing down from there.
 */
function renderFlightChecklist(flights, container) {
  container.innerHTML = "";

  if (flights.length === 0) {
    container.textContent = "No flights found.";
    return;
  }

  const controls = document.createElement("div");
  controls.className = "select-all-controls";
  const selectAllBtn = document.createElement("button");
  selectAllBtn.type = "button";
  selectAllBtn.textContent = "Select all";
  const deselectAllBtn = document.createElement("button");
  deselectAllBtn.type = "button";
  deselectAllBtn.textContent = "Deselect all";
  controls.appendChild(selectAllBtn);
  controls.appendChild(deselectAllBtn);
  container.appendChild(controls);

  const list = document.createElement("div");
  list.className = "flight-checklist";
  const checkboxes = [];

  for (const flight of flights) {
    const label = document.createElement("label");
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = true; // selected by default
    checkbox.dataset.layerId = flight.layerId;
    checkbox.dataset.coords = JSON.stringify(flight.coordinates);
    checkboxes.push(checkbox);

    checkbox.addEventListener("change", () => {
      if (checkbox.checked) {
        setFlightLayer(map, flight.layerId, flight.coordinates, colorFor(flight.layerId));
      } else {
        removeFlightLayer(map, flight.layerId);
      }
      refitToVisible();
    });

    label.appendChild(checkbox);
    label.appendChild(document.createTextNode(flight.label));
    if (flight.duration || flight.distance) {
      const small = document.createElement("small");
      small.style.color = "#666";
      small.style.marginLeft = "0.3em";
      small.textContent = `(${flight.duration ?? "?"}, ${flight.distance ?? "?"})`;
      label.appendChild(small);
    }
    list.appendChild(label);

    // Drawn immediately since the default state is checked.
    setFlightLayer(map, flight.layerId, flight.coordinates, colorFor(flight.layerId));
  }
  container.appendChild(list);

  selectAllBtn.addEventListener("click", () => {
    for (const cb of checkboxes) {
      if (!cb.checked) {
        cb.checked = true;
        cb.dispatchEvent(new Event("change"));
      }
    }
  });
  deselectAllBtn.addEventListener("click", () => {
    for (const cb of checkboxes) {
      if (cb.checked) {
        cb.checked = false;
        cb.dispatchEvent(new Event("change"));
      }
    }
  });

  fitToCoordinates(map, flights.map((f) => f.coordinates));
}

/**
 * One project (or standalone/ungrouped location) in the sidebar: a
 * header button that, on first click, loads every flight in `entries`
 * and shows them as an already-all-selected checklist right below it —
 * "automatically opened," per the whole point of picking a project.
 * Clicking the header again just collapses/expands the panel; it
 * doesn't touch the checkbox/map state either way.
 */
function appendProjectPanel(displayName, entries, wrapper) {
  const header = document.createElement("button");
  header.className = "project-header";
  header.textContent = displayName;

  const panel = document.createElement("div");
  panel.className = "project-panel";
  panel.style.display = "none";

  header.addEventListener("click", async () => {
    const isOpen = panel.style.display !== "none";
    panel.style.display = isOpen ? "none" : "block";
    if (!isOpen && panel.dataset.loaded !== "true") {
      panel.dataset.loaded = "true";
      panel.textContent = "Loading…";
      const flights = await loadFlightsForEntries(entries);
      renderFlightChecklist(flights, panel);
    }
  });

  wrapper.appendChild(header);
  wrapper.appendChild(panel);
}

/**
 * Sidebar as a flat list of Projects — no folder browsing, no separate
 * date level. Clicking a project name is the only navigation needed.
 */
function renderProjectTree({ projects, ungrouped }) {
  locationList.innerHTML = "";

  for (const project of projects) {
    const wrapper = document.createElement("div");
    wrapper.className = "project-entry";
    appendProjectPanel(project.projectName, project.dates, wrapper);
    locationList.appendChild(wrapper);
  }

  if (ungrouped.length > 0) {
    const heading = document.createElement("div");
    heading.className = "ungrouped-heading";
    heading.textContent = "Other (not in a project folder)";
    locationList.appendChild(heading);

    for (const entry of ungrouped) {
      const wrapper = document.createElement("div");
      wrapper.className = "project-entry";
      appendProjectPanel(`${entry.folderName} — ${formatDateKey(entry.dateKey)}`, [entry], wrapper);
      locationList.appendChild(wrapper);
    }
  }
}

clearAllBtn.addEventListener("click", () => {
  for (const cb of document.querySelectorAll(".flight-checklist input:checked")) {
    cb.checked = false;
    cb.dispatchEvent(new Event("change"));
  }
  // Collapse every project's panel too, not just clear its selections —
  // keeps the sidebar navigable as more projects pile up, rather than
  // leaving every previously-opened one expanded.
  for (const panel of document.querySelectorAll(".project-panel")) {
    panel.style.display = "none";
  }
});

// This page always points at one fixed location — end users never pick
// or change a folder. If that location ever needs to change, it's a code
// change here, not something exposed as a UI option.
const DESTINATION_HINT = String.raw`O:\Flight Logs Output (Do Not Put Files Here)\KMZs`;

function renderConnected(handle) {
  connectRow.textContent = `Connected: ${handle.name}`;
}

async function loadFromHandle(handle) {
  renderConnected(handle);
  locationList.textContent = "Scanning…";
  const entries = await collectKmzFiles(handle);
  const tree = buildProjectTree(entries);
  if (tree.projects.length === 0 && tree.ungrouped.length === 0) {
    locationList.textContent = `No .kmz files found — make sure "${DESTINATION_HINT}" was selected.`;
    return;
  }
  renderProjectTree(tree);
}

function renderChooseButton() {
  connectRow.innerHTML = "";
  const btn = document.createElement("button");
  btn.textContent = "Connect";
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

  // Only shown before the one-time setup — the browser's own folder
  // dialog can't be skipped or pre-navigated from JS, but this is the
  // only time it's needed; every visit after this just shows projects,
  // with no further folder/connection choices exposed anywhere.
  const hint = document.createElement("p");
  hint.style.cssText = "margin: 0.5rem 0 0; font-size: 0.85em; color: #666;";
  hint.textContent = `One-time setup: select "${DESTINATION_HINT}". Your browser will remember it after this.`;
  connectRow.appendChild(hint);
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
