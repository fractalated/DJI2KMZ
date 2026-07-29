// File System Access API + IndexedDB persistence for the flight log
// viewer. This reads .kmz files directly from a folder the user picks
// (typically a mapped network drive) entirely client-side — nothing is
// ever uploaded anywhere. Chromium-only (Chrome/Edge); the page this
// module supports is already scoped to that constraint.

const DB_NAME = "dji2kmz-viewer";
const STORE_NAME = "handles";
const HANDLE_KEY = "rootDir";

function openDb() {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, 1);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE_NAME);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

async function saveDirHandle(handle, key) {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readwrite");
    tx.objectStore(STORE_NAME).put(handle, key);
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
  });
}

async function loadDirHandle(key) {
  const db = await openDb();
  return new Promise((resolve, reject) => {
    const tx = db.transaction(STORE_NAME, "readonly");
    const req = tx.objectStore(STORE_NAME).get(key);
    req.onsuccess = () => resolve(req.result ?? null);
    req.onerror = () => reject(req.error);
  });
}

/**
 * Must be called from a click handler — the picker enforces this.
 * `mode` is "read" (viewer/logbook, browsing already-converted files) or
 * "readwrite" (the import page, writing KMZs/pilot log directly into a
 * destination folder). `key` distinguishes which remembered handle this
 * is — the import page's destination folder is a different folder than
 * the viewer's, so they're persisted under different keys even though
 * both live in this same IndexedDB store.
 */
export async function pickDirectory(mode = "read", key = HANDLE_KEY) {
  const handle = await window.showDirectoryPicker({ mode });
  try {
    await saveDirHandle(handle, key);
  } catch (err) {
    // Persistence is a convenience, not a requirement — losing it (e.g.
    // private browsing, storage disabled) shouldn't block using the
    // folder for this session.
    console.warn("Could not persist folder access for next visit:", err);
  }
  return handle;
}

/**
 * Call on page load. Returns { handle, granted } if a folder was
 * previously picked, or null if none has ever been chosen. `granted`
 * false means permission needs re-confirming — show a "Reconnect" button
 * and call requestPermission() from ITS click handler (queryPermission is
 * gesture-free, but requestPermission is not).
 */
export async function restoreDirectory(mode = "read", key = HANDLE_KEY) {
  const handle = await loadDirHandle(key);
  if (!handle) return null;
  const granted = (await handle.queryPermission({ mode })) === "granted";
  return { handle, granted };
}

/** Call from a user-gesture handler (e.g. a "Reconnect" button's click). */
export async function requestPermission(handle, mode = "read") {
  return (await handle.requestPermission({ mode })) === "granted";
}

/**
 * Recursively walks a directory, returning every .kmz file found.
 * folderKey is the full relative path from the root (unique even if two
 * folders share a leaf name); folderName is just the leaf, for display.
 */
export async function collectKmzFiles(dirHandle, folderKey = "", folderName = dirHandle.name) {
  const results = [];
  for await (const [name, handle] of dirHandle.entries()) {
    if (handle.kind === "file") {
      if (name.toLowerCase().endsWith(".kmz")) {
        results.push({
          file: await handle.getFile(),
          folderKey: folderKey || dirHandle.name,
          folderName,
        });
      }
    } else if (handle.kind === "directory") {
      const childKey = folderKey ? `${folderKey}/${name}` : name;
      results.push(...(await collectKmzFiles(handle, childKey, name)));
    }
  }
  return results;
}
