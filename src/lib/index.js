// Pure helpers ported from the original reader: filename parsing, sorting,
// and building the chapter index/tree. No DOM, no framework — easy to test.

export function numFromName(name) {
  const m = name.match(/ch\.?(\d+)|(\d+)\./);
  return m ? parseInt(m[1] || m[2], 10) : 999;
}

export function labelFromName(name) {
  return name
    .replace(/\.md$/i, '')
    .replace(/^ch\.?\d+[-_. ]*/i, '') // strip ch01- / ch01_ prefix
    .replace(/[-_]/g, ' ')
    .replace(/\b\w/g, (c) => c.toUpperCase());
}

export function shortTitle(name) {
  const num = numFromName(name);
  const label = labelFromName(name);
  return num < 999 ? `${num}. ${label}` : label;
}

export function sortGroups(gm) {
  const sorted = {};
  Object.keys(gm)
    .sort((a, b) => {
      const na = parseInt(a.replace(/[^0-9]/g, '')) || 0;
      const nb = parseInt(b.replace(/[^0-9]/g, '')) || 0;
      return na - nb;
    })
    .forEach((k) => {
      sorted[k] = gm[k];
    });
  return sorted;
}

function sortGroupItems(groupMeta) {
  for (const g in groupMeta) {
    const hasCh = groupMeta[g].some((i) => /ch\.?(\d+)/i.test(i.name));
    groupMeta[g].sort((a, b) =>
      hasCh ? numFromName(a.name) - numFromName(b.name) : a.name.localeCompare(b.name)
    );
  }
}

/**
 * Build { folderName, folderMeta, groupMeta } from native Rust records
 * `{ path, name, content }`. `diskPath` keeps the full path under the root
 * (for watcher correlation + write_file + progress keys); `path` is
 * group-stripped to match the renderer's expectations.
 */
export function buildIndexFromRecords(name, records) {
  const folderMeta = [];
  const groupMeta = {};
  for (const r of records) {
    const parts = r.path.split('/');
    const folderPrefix = parts[0];
    if (folderPrefix.startsWith('.')) continue;
    const isGrouped = parts.length > 1;
    const relativePath = isGrouped ? parts.slice(1).join('/') : r.name;
    const item = {
      path: relativePath,
      diskPath: r.path,
      name: r.name,
      content: r.content,
      grouped: isGrouped,
    };
    folderMeta.push(item);
    if (isGrouped) {
      (groupMeta[folderPrefix] ||= []).push(item);
    }
  }
  const sorted = sortGroups(groupMeta);
  sortGroupItems(sorted);
  return { folderName: name || 'Selected Folder', folderMeta, groupMeta: sorted };
}

/** Group files into a nested folder tree (folders first, then files by number). */
export function buildFolderTree(items) {
  const root = { name: '', children: {} };
  for (const item of items) {
    const parts = item.path.split('/');
    let node = root;
    for (let i = 0; i < parts.length - 1; i++) {
      const part = parts[i];
      if (!node.children[part]) {
        node.children[part] = {
          name: part,
          folderPath: parts.slice(0, i + 1).join('/'),
          children: {},
        };
      }
      node = node.children[part];
    }
    const fileName = parts[parts.length - 1];
    node.children[fileName] = { name: fileName, path: item.path, item };
  }

  function treeToArray(node) {
    const folders = [];
    const files = [];
    for (const child of Object.values(node.children)) {
      (child.path ? files : folders).push(child);
    }
    folders.sort((a, b) => a.name.localeCompare(b.name));
    files.sort((a, b) => {
      const an = numFromName(a.name);
      const bn = numFromName(b.name);
      return an !== bn ? an - bn : a.name.localeCompare(b.name);
    });
    const result = [...folders, ...files];
    for (const child of result) {
      if (!child.path) child.children = treeToArray(child);
    }
    return result;
  }
  return treeToArray(root);
}
