import { pickDirectory, restoreDirectory, requestPermission, collectKmzFiles } from "../shared/fs.js";
import { buildProjectTree, formatDateKey, formatDateFolder, loadPlacemarks } from "../shared/grouping.js";
import { initMap, setFlightLayer, removeFlightLayer, fitToCoordinates } from "./map.js";

const connectRow = document.getElementById("connectRow");
const locationList = document.getElementById("locationList");
const map = initMap("map");

// A few distinguishable line colors, cycled per visible flight so
// multiple simultaneously-checked flights (within or across locations)
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
 * Renders a checklist of every flight across all of `entries` (a project's
 * flights span multiple date-folder entries; a single date is just a
 * one-element array) — each entry's flights load independently, so one
 * date failing to load doesn't block the rest.
 */
async function renderChecklist(entries, container) {
  container.innerHTML = "Loading…";
  const perEntryPlacemarks = [];
  for (const entry of entries) {
    try {
      perEntryPlacemarks.push(await loadPlacemarks(entry));
    } catch (err) {
      console.warn(`Failed to load ${entry.folderKey}:`, err);
      perEntryPlacemarks.push([]);
    }
  }

  container.innerHTML = "";
  const list = document.createElement("div");
  list.className = "flight-checklist";

  entries.forEach((entry, i) => {
    for (const pm of perEntryPlacemarks[i]) {
      const layerId = `${entry.folderKey}::${pm.name}`;
      const label = document.createElement("label");
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.dataset.layerId = layerId;
      checkbox.dataset.coords = JSON.stringify(pm.coordinates);

      checkbox.addEventListener("change", () => {
        if (checkbox.checked) {
          setFlightLayer(map, layerId, pm.coordinates, colorFor(layerId));
        } else {
          removeFlightLayer(map, layerId);
        }
        refitToVisible();
      });

      label.appendChild(checkbox);
      label.appendChild(document.createTextNode(pm.name));
      if (pm.meta.startTime) {
        const small = document.createElement("small");
        small.style.color = "#666";
        small.style.marginLeft = "0.3em";
        small.textContent = `(${pm.meta.duration ?? "?"}, ${pm.meta.distance ?? "?"})`;
        label.appendChild(small);
      }
      list.appendChild(label);
    }
  });

  container.appendChild(list);
}

/** A button that toggles a lazily-loaded checklist of `entries` directly below it. */
function appendToggleableChecklist(entries, button, parent) {
  const detail = document.createElement("div");
  detail.style.display = "none";

  button.addEventListener("click", () => {
    const isOpen = detail.style.display !== "none";
    detail.style.display = isOpen ? "none" : "block";
    if (!isOpen && detail.dataset.loaded !== "true") {
      detail.dataset.loaded = "true";
      renderChecklist(entries, detail);
    }
  });

  parent.appendChild(button);
  parent.appendChild(detail);
}

/**
 * A button that draws every flight across `entries` straight onto the
 * map — no checklist to click through — and hides them all again on a
 * second click. Used for a project's own header, where the flight count
 * across every date can grow large enough that a full checklist becomes
 * unwieldy; a project only ever needs an all-or-nothing view like this,
 * unlike a single date (still a checklist, so individual flights within
 * one day stay toggleable).
 */
function appendMapOnlyToggle(entries, button, parent) {
  let flights = null; // [{layerId, coords}], loaded lazily on first click
  let visible = false;
  const label = button.textContent;

  button.addEventListener("click", async () => {
    if (flights === null) {
      button.disabled = true;
      button.textContent = `${label} (loading…)`;
      flights = [];
      for (const entry of entries) {
        let placemarks;
        try {
          placemarks = await loadPlacemarks(entry);
        } catch (err) {
          console.warn(`Failed to load ${entry.folderKey}:`, err);
          placemarks = [];
        }
        for (const pm of placemarks) {
          flights.push({ layerId: `${entry.folderKey}::${pm.name}`, coords: pm.coordinates });
        }
      }
      button.textContent = label;
      button.disabled = false;
    }

    visible = !visible;
    button.classList.toggle("active", visible);
    for (const { layerId, coords } of flights) {
      if (visible) {
        setFlightLayer(map, layerId, coords, colorFor(layerId));
      } else {
        removeFlightLayer(map, layerId);
      }
    }
    if (visible && flights.length > 0) {
      fitToCoordinates(map, flights.map((f) => f.coords));
    }
  });

  parent.appendChild(button);
}

/**
 * Sidebar as a Project -> Date tree. Clicking a project name draws every
 * flight across all of its dates straight onto the map (click again to
 * hide them); clicking a date underneath instead shows that day's
 * checklist, for toggling individual flights within it.
 */
function renderProjectTree({ projects, ungrouped }) {
  locationList.innerHTML = "";

  for (const project of projects) {
    const wrapper = document.createElement("div");
    wrapper.className = "project-entry";

    const header = document.createElement("button");
    header.className = "project-header";
    header.textContent = project.projectName;
    appendMapOnlyToggle(project.dates, header, wrapper);

    const dateList = document.createElement("div");
    dateList.className = "date-list";
    for (const dateEntry of project.dates) {
      const dateWrapper = document.createElement("div");
      dateWrapper.className = "date-entry";
      const dateButton = document.createElement("button");
      dateButton.textContent = formatDateFolder(dateEntry.dateFolder);
      appendToggleableChecklist([dateEntry], dateButton, dateWrapper);
      dateList.appendChild(dateWrapper);
    }
    wrapper.appendChild(dateList);

    locationList.appendChild(wrapper);
  }

  if (ungrouped.length > 0) {
    const heading = document.createElement("div");
    heading.className = "ungrouped-heading";
    heading.textContent = "Other (not in a project/date folder)";
    locationList.appendChild(heading);

    for (const entry of ungrouped) {
      const wrapper = document.createElement("div");
      wrapper.className = "location-entry";
      const button = document.createElement("button");
      button.textContent = `${entry.folderName} — ${formatDateKey(entry.dateKey)}`;
      appendToggleableChecklist([entry], button, wrapper);
      locationList.appendChild(wrapper);
    }
  }
}

function renderConnected(handle) {
  connectRow.innerHTML = `Connected: <strong>${handle.name}</strong> `;
  const btn = document.createElement("button");
  btn.textContent = "Change Folder";
  btn.addEventListener("click", async () => {
    try {
      const newHandle = await pickDirectory();
      await loadFromHandle(newHandle);
    } catch (err) {
      if (err.name !== "AbortError") {
        connectRow.textContent = `Error: ${err.message ?? err}`;
      }
    }
  });
  connectRow.appendChild(btn);
}

async function loadFromHandle(handle) {
  renderConnected(handle);
  locationList.textContent = "Scanning folder…";
  const entries = await collectKmzFiles(handle);
  const tree = buildProjectTree(entries);
  if (tree.projects.length === 0 && tree.ungrouped.length === 0) {
    locationList.textContent = "No .kmz files found in this folder — point this at the KMZs folder inside your destination.";
    return;
  }
  renderProjectTree(tree);
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
