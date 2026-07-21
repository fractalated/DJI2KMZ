import { pickDirectory, restoreDirectory, requestPermission, collectKmzFiles } from "../shared/fs.js";
import { buildLocationEntries, formatDateKey, loadPlacemarks } from "../shared/grouping.js";
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

async function renderChecklist(entry, container) {
  container.innerHTML = "Loading…";
  let placemarks;
  try {
    placemarks = await loadPlacemarks(entry);
  } catch (err) {
    container.textContent = `Failed to load: ${err.message ?? err}`;
    return;
  }

  container.innerHTML = "";
  const list = document.createElement("div");
  list.className = "flight-checklist";

  for (const pm of placemarks) {
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

  container.appendChild(list);
}

function renderLocationList(locations) {
  locationList.innerHTML = "";
  for (const entry of locations) {
    const wrapper = document.createElement("div");
    wrapper.className = "location-entry";

    const button = document.createElement("button");
    button.textContent = `${entry.folderName} — ${formatDateKey(entry.dateKey)}`;

    const detail = document.createElement("div");
    detail.style.display = "none";

    button.addEventListener("click", () => {
      const isOpen = detail.style.display !== "none";
      detail.style.display = isOpen ? "none" : "block";
      if (!isOpen && detail.dataset.loaded !== "true") {
        detail.dataset.loaded = "true";
        renderChecklist(entry, detail);
      }
    });

    wrapper.appendChild(button);
    wrapper.appendChild(detail);
    locationList.appendChild(wrapper);
  }
}

async function loadFromHandle(handle) {
  connectRow.innerHTML = `Connected: <strong>${handle.name}</strong>`;
  locationList.textContent = "Scanning folder…";
  const entries = await collectKmzFiles(handle);
  const locations = buildLocationEntries(entries);
  if (locations.length === 0) {
    locationList.textContent = "No .kmz files found in this folder.";
    return;
  }
  renderLocationList(locations);
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
