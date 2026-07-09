(function () {
  "use strict";

  const manifestNode = document.querySelector("script[data-hisi-docs-manifest]");
  if (!manifestNode) {
    return;
  }

  let manifest;
  try {
    manifest = JSON.parse(manifestNode.textContent || "{}");
  } catch (_err) {
    return;
  }

  const chips = manifest.chips || {};
  const versions = manifest.versions || {};
  const chipMap = chips.chips || {};
  const selectable = chips.selectable || Object.keys(chipMap);
  const defaultChip = chips.default || selectable[0] || "ws63";
  const versionList = versions.versions || [];
  const defaultVersion = versions.default || "latest";

  function paramsChip() {
    const params = new URLSearchParams(window.location.search);
    const chip = params.get("chip");
    return chip && chipMap[chip] ? chip : null;
  }

  function currentChip() {
    const urlChip = paramsChip();
    if (urlChip) {
      localStorage.setItem("hisi-docs-chip", urlChip);
      return urlChip;
    }
    const stored = localStorage.getItem("hisi-docs-chip");
    return stored && chipMap[stored] ? stored : defaultChip;
  }

  function currentVersion() {
    const segments = window.location.pathname.split("/").filter(Boolean);
    const known = versionList.map((version) => version.id);
    for (const segment of segments) {
      if (known.includes(segment)) {
        return segment;
      }
    }
    return defaultVersion;
  }

  function versionById(id) {
    return versionList.find((version) => version.id === id) || versionList[0] || null;
  }

  function apiBaseForVersion(versionId) {
    const version = versionById(versionId) || versionById(defaultVersion);
    return version && version.api_base ? version.api_base : "/hisi-riscv-rs/api/latest/";
  }

  function applyChip(chip) {
    const data = chipMap[chip] || chipMap[defaultChip] || {};
    document.documentElement.dataset.hisiChip = chip;

    document.querySelectorAll("[data-hisi-chip]").forEach((node) => {
      node.hidden = node.getAttribute("data-hisi-chip") !== chip;
    });

    document.querySelectorAll("[data-hisi-chip-field]").forEach((node) => {
      const field = node.getAttribute("data-hisi-chip-field");
      if (field && Object.prototype.hasOwnProperty.call(data, field)) {
        node.textContent = String(data[field]);
      }
    });

    document.querySelectorAll("[data-hisi-api-link]").forEach((node) => {
      const path = node.getAttribute("data-hisi-api-link") || "";
      const apiChip = data.api_chip || chip;
      const base = apiBaseForVersion(currentVersion()).replace(/\/$/, "");
      node.setAttribute("href", `${base}/${apiChip}/${path.replace(/^\//, "")}`);
    });

    document.querySelectorAll("[data-hisi-chip-note]").forEach((node) => {
      node.textContent = data.notes || "";
    });
  }

  function relativePagePath() {
    const segments = window.location.pathname.split("/").filter(Boolean);
    const ids = versionList.map((version) => version.id);
    const index = segments.findIndex((segment) => ids.includes(segment));
    if (index >= 0) {
      return segments.slice(index + 1).join("/") || "index.html";
    }
    return segments.slice(-1)[0] || "index.html";
  }

  function switchVersion(versionId) {
    const version = versionById(versionId);
    if (!version) {
      return;
    }
    const base = (version.book_base || "/").replace(/\/$/, "");
    const page = relativePagePath();
    const chip = currentChip();
    window.location.href = `${base}/${page}?chip=${encodeURIComponent(chip)}`;
  }

  function renderControls() {
    document.querySelectorAll("[data-hisi-docs-controls]").forEach((root) => {
      root.innerHTML = "";

      const chipLabel = document.createElement("label");
      chipLabel.className = "hisi-docs-control";
      chipLabel.textContent = "Chip";
      const chipSelect = document.createElement("select");
      selectable.forEach((chip) => {
        const option = document.createElement("option");
        option.value = chip;
        option.textContent = (chipMap[chip] && chipMap[chip].display_name) || chip;
        chipSelect.appendChild(option);
      });
      chipSelect.value = currentChip();
      chipSelect.addEventListener("change", () => {
        localStorage.setItem("hisi-docs-chip", chipSelect.value);
        applyChip(chipSelect.value);
      });
      chipLabel.appendChild(chipSelect);

      const versionLabel = document.createElement("label");
      versionLabel.className = "hisi-docs-control";
      versionLabel.textContent = "Version";
      const versionSelect = document.createElement("select");
      versionList.forEach((version) => {
        const option = document.createElement("option");
        option.value = version.id;
        option.textContent = version.label || version.id;
        versionSelect.appendChild(option);
      });
      versionSelect.value = currentVersion();
      versionSelect.addEventListener("change", () => switchVersion(versionSelect.value));
      versionLabel.appendChild(versionSelect);

      const note = document.createElement("span");
      note.className = "hisi-docs-chip-note";
      note.setAttribute("data-hisi-chip-note", "");

      root.append(chipLabel, versionLabel, note);
    });
  }

  renderControls();
  applyChip(currentChip());
})();
