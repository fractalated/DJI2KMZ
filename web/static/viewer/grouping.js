// Groups collected .kmz files by folder and sorts by date — all derived
// from filenames alone (DJI2KMZ's own naming contract), so building the
// sidebar never requires opening/parsing any file content. Matches
// core/src/naming.rs exactly:
//   individual: "{MM-DD-YYYY}_{HH-MM}_{cleaned_folder_name}.kmz"
//   merged:     "{cleaned_folder_name}_Flight_Logs_{MM-DD-YYYY}.kmz"
//           or  "{cleaned_folder_name}_Flight_Logs_{MM-DD--MM-DD-YYYY}.kmz"
// In both merged shapes the date immediately before ".kmz" is always a
// full MM-DD-YYYY, so one regex covers both.

const INDIVIDUAL_PREFIX_RE = /^(\d{2})-(\d{2})-(\d{4})_/;
const MERGED_SUFFIX_RE = /(\d{2})-(\d{2})-(\d{4})\.kmz$/i;

// clean_folder_name (core/src/naming.rs) strips "flight"/"flights"/
// "log"/"logs" from folder names, so a legitimate cleaned folder name can
// never itself contain this substring — safe, unambiguous marker.
export function isMerged(name) {
  return name.includes("_Flight_Logs_");
}

function dateKeyFromFilename(name) {
  const m = isMerged(name) ? name.match(MERGED_SUFFIX_RE) : name.match(INDIVIDUAL_PREFIX_RE);
  if (!m) return null;
  const [, mm, dd, yyyy] = m;
  return `${yyyy}${mm}${dd}`; // sortable YYYYMMDD
}

/**
 * entries: [{file, folderKey, folderName}] from collectKmzFiles.
 * Returns sidebar-ready location entries, sorted newest-first.
 */
export function buildLocationEntries(entries) {
  const byFolder = new Map();
  for (const e of entries) {
    if (!byFolder.has(e.folderKey)) {
      byFolder.set(e.folderKey, { folderName: e.folderName, files: [] });
    }
    byFolder.get(e.folderKey).files.push(e.file);
  }

  const locations = [];
  for (const [folderKey, { folderName, files }] of byFolder) {
    const merged = files.find((f) => isMerged(f.name)) ?? null;
    const dateKey = merged
      ? dateKeyFromFilename(merged.name)
      : files
          .map((f) => dateKeyFromFilename(f.name))
          .filter(Boolean)
          .sort()
          .at(-1); // latest flight represents the whole batch

    locations.push({
      folderKey,
      folderName,
      dateKey: dateKey ?? "00000000", // unrecognized names sink to one end, never crash the sort
      mergedFile: merged,
      individualFiles: merged ? [] : files, // only used as the fallback path
    });
  }

  locations.sort((a, b) => b.dateKey.localeCompare(a.dateKey));
  return locations;
}

/** "20260615" -> "06/15/2026", for sidebar display. */
export function formatDateKey(dateKey) {
  if (!dateKey || dateKey === "00000000") return "Unknown date";
  const yyyy = dateKey.slice(0, 4);
  const mm = dateKey.slice(4, 6);
  const dd = dateKey.slice(6, 8);
  return `${mm}/${dd}/${yyyy}`;
}
