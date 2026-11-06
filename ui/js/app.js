const directionButtons = document.querySelectorAll("[data-direction]");
const sourceLabel = document.querySelector("#source-label");
const sourceKind = document.querySelector("#source-kind");
const sourcePath = document.querySelector("#source-path");
const destinationLabel = document.querySelector("#destination-label");
const destinationKind = document.querySelector("#destination-kind");
const destinationPath = document.querySelector("#destination-path");
const views = document.querySelectorAll("[data-view]");
const navigationLinks = document.querySelectorAll(".navigation a[href^='#']");
const viewLinks = document.querySelectorAll("a[href^='#']");
const breadcrumb = document.querySelector("#breadcrumb");
const profileDialog = document.querySelector("#profile-dialog");
const profileForm = document.querySelector("#profile-form");
const gitRepositoryList = document.querySelector("#git-repository-list");
const profileList = document.querySelector("#connection-list");
const profileEmpty = document.querySelector("#connection-empty");
const profileMessage = document.querySelector("#profile-message");
const profileFormError = document.querySelector("#profile-form-error");
const profileTotal = document.querySelector("#profile-total");
const profileNavCount = document.querySelector("#profile-nav-count");
const dashboardProfile = document.querySelector("#profile");
const profileSaveButton = document.querySelector("#save-profile-button");
const previewButton = document.querySelector("#preview-button");
const startSyncButton = document.querySelector("#start-sync-button");
const transferStatus = document.querySelector("#transfer-status");
const transferOutput = document.querySelector("#transfer-output");
const agentEventList = document.querySelector("#agent-event-list");
const agentLiveState = document.querySelector("#agent-live-state");
const agentIntegrationStatus = document.querySelector("#agent-integration-status");
const installAgentMcpButton = document.querySelector("#install-agent-mcp-button");
const installAgentHooksButton = document.querySelector("#install-agent-hooks-button");
const agentClientSelect = document.querySelector("#agent-client-select");
const agentClientHelp = document.querySelector("#agent-client-help");
const showAgentMcpConfigButton = document.querySelector("#show-agent-mcp-config-button");
const agentMcpConfig = document.querySelector("#agent-mcp-config");
const watchSessionPanel = document.querySelector("#watch-session-panel");
const watchSessionStatus = document.querySelector("#watch-session-status");
const stopWatchButton = document.querySelector("#stop-watch-button");
const transferDialog = document.querySelector("#transfer-dialog");
const previewOutput = document.querySelector("#preview-output");
const previewSummary = document.querySelector("#transfer-dialog-summary");
const confirmSyncButton = document.querySelector("#confirm-sync-button");
const startWatchButton = document.querySelector("#start-watch-button");
const compareSourceSelect = document.querySelector("#compare-source-select");
const compareFileButton = document.querySelector("#compare-file-button");
const diffFormatSelect = document.querySelector("#diff-format");
const fileCompareStatus = document.querySelector("#file-compare-status");
const fileDiffOutput = document.querySelector("#file-diff-output");
const sourceHelp = document.querySelector("#source-help");
const sourceTextarea = document.querySelector("#source-path");
const sourceLabelRow = document.querySelector("#source-label-row");
const folderBrowserPanel = document.querySelector("#folder-browser-panel");
const selectedSourceList = document.querySelector("#selected-source-list");
const browseFilesButton = document.querySelector("#browse-files-button");
const browseFolderButton = document.querySelector("#browse-folder-button");
const browseDestinationButton = document.querySelector("#browse-destination-button");
const remoteBrowserPanel = document.querySelector("#remote-browser-panel");
const remoteBrowserPathInput = document.querySelector("#remote-browser-path-input");
const remoteBrowserList = document.querySelector("#remote-browser-list");
const remoteBrowserStatus = document.querySelector("#remote-browser-status");
const remoteBrowserUpButton = document.querySelector("#remote-browser-up");
const remoteBrowserOpenButton = document.querySelector("#remote-browser-open");
const remoteBrowserSelectButton = document.querySelector("#remote-browser-select");
const folderBrowserPath = document.querySelector("#folder-browser-path");
const folderBrowserList = document.querySelector("#folder-browser-list");
const folderBrowserStatus = document.querySelector("#folder-browser-status");
const folderBrowserUpButton = document.querySelector("#folder-browser-up");
const addCurrentFolderButton = document.querySelector("#add-current-folder");
const addFolderSelectionButton = document.querySelector("#add-folder-selection");
const folderSelectionCount = document.querySelector("#folder-selection-count");
const profileProfiles = [];
const desktopApi = window.__TAURI__;
let currentPreview = null;
let currentPreviewInput = null;
let currentFileComparison = null;
let currentTransferId = null;
let activeWatcherId = null;
let pendingWatcherStart = false;
let pendingWatchStatus = null;
let watchEventsReady = false;
let latestAgentActivitySignature = "";
let activeDirection = "push";
let folderBrowserRoot = "";
let folderBrowserCurrent = "";
let remoteBrowserRoot = "";
let remoteBrowserCurrent = "";
const folderBrowserSelection = new Map();

function showWatchStatus(payload) {
  watchSessionPanel.hidden = false;
  watchSessionStatus.textContent = payload.message;
  watchSessionStatus.dataset.state = payload.state;
  const isActive = payload.state !== "stopped";
  stopWatchButton.hidden = !isActive;
  stopWatchButton.disabled = payload.state === "starting" || payload.state === "syncing";
  if (!isActive) activeWatcherId = null;
  updateTransferControls();
}

if (desktopApi?.event?.listen) {
  desktopApi.event.listen("watch-status", ({ payload }) => {
    if (pendingWatcherStart) {
      pendingWatchStatus = payload;
      return;
    }
    if (payload.watcherId === activeWatcherId) showWatchStatus(payload);
  }).then(() => {
    watchEventsReady = true;
    updateTransferControls();
  }).catch((error) => {
    watchSessionPanel.hidden = false;
    watchSessionStatus.textContent = `Could not listen for Watch & Sync updates: ${String(error)}`;
    watchSessionStatus.dataset.state = "error";
  });
}

const viewTitles = {
  dashboard: "Dashboard",
  connections: "Connection",
  activity: "Activity",
  "agent-activity": "AI Agent",
};

const directions = {
  push: {
    sourceLabel: "Choose files to transfer",
    sourceKind: "LOCAL FILES",
    sourcePlaceholder: "Select one or more files or folders",
    sourceHelp: "Browse a folder to select individual files or folders, or use Select files for multiple files.",
    destinationLabel: "Server destination folder",
    destinationKind: "REMOTE",
    destinationPlaceholder: "/absolute/server/folder",
    localSources: true,
  },
  pull: {
    sourceLabel: "Remote source paths",
    sourceKind: "REMOTE",
    sourcePlaceholder: "/absolute/remote/file/or/folder\n~/another/remote/file",
    sourceHelp: "Enter one absolute remote path per line. Multiple remote files and folders are supported.",
    destinationLabel: "Local destination folder",
    destinationKind: "THIS DEVICE",
    destinationPlaceholder: "/absolute/local/folder",
    localSources: false,
  },
};

function setDirection(direction) {
  const config = directions[direction];
  if (!config) return;
  const directionChanged = activeDirection !== direction;
  activeDirection = direction;
  invalidatePreview();
  if (directionChanged) {
    sourcePath.value = "";
    destinationPath.value = "";
    transferOutput.hidden = true;
  }

  sourceLabel.textContent = config.sourceLabel;
  sourceKind.textContent = config.sourceKind;
  sourcePath.placeholder = config.sourcePlaceholder;
  sourceHelp.textContent = config.sourceHelp;
  sourcePath.hidden = config.localSources;
  sourceLabelRow.hidden = false;
  folderBrowserPanel.hidden = !config.localSources;
  sourceHelp.hidden = config.localSources;
  selectedSourceList.hidden = !config.localSources || sourcePaths().length === 0;
  destinationLabel.textContent = config.destinationLabel;
  destinationKind.textContent = config.destinationKind;
  destinationKind.classList.toggle("path-kind-remote", config.destinationKind === "REMOTE");
  sourceKind.classList.toggle("path-kind-remote", config.sourceKind === "REMOTE");
  destinationPath.placeholder = config.destinationPlaceholder;
  browseFilesButton.hidden = !config.localSources;
  browseFolderButton.hidden = !config.localSources;
  browseDestinationButton.setAttribute(
    "aria-label",
    config.localSources ? "Browse remote destination folders" : "Choose local destination folder",
  );
  remoteBrowserPanel.hidden = config.destinationKind !== "REMOTE";
  if (config.destinationKind !== "REMOTE") {
    remoteBrowserRoot = "";
    remoteBrowserCurrent = "";
  }
  updateTransferControls();

  for (const button of directionButtons) {
    const isSelected = button.dataset.direction === direction;
    button.classList.toggle("is-selected", isSelected);
    button.setAttribute("aria-pressed", String(isSelected));
  }
}

for (const button of directionButtons) {
  button.addEventListener("click", () => setDirection(button.dataset.direction));
}

function invalidatePreview() {
  currentPreview = null;
  currentPreviewInput = null;
  startSyncButton.disabled = true;
}

function sourcePaths() {
  return sourceTextarea.value.split(/\r?\n/).map((path) => path.trim()).filter(Boolean);
}

function selectedExclusions() {
  return [...document.querySelectorAll(".exclusion-options input:checked")].map((checkbox) => checkbox.value);
}

function updateTransferControls() {
  const canTest = Boolean(dashboardProfile.value);
  const canPreview = canTest && sourcePaths().length > 0 && destinationPath.value.trim().length > 0;
  const watchLocked = Boolean(activeWatcherId || pendingWatcherStart);
  browseFilesButton.disabled = watchLocked || !directions[activeDirection].localSources;
  browseFolderButton.disabled = watchLocked || !directions[activeDirection].localSources;
  browseDestinationButton.disabled =
    watchLocked || (activeDirection === "push" && !canTest);
  previewButton.disabled = watchLocked || !canPreview;
  startSyncButton.disabled = watchLocked || !currentPreview;
  startWatchButton.hidden =
    watchLocked ||
    !currentPreview ||
    activeDirection !== "push" ||
    sourcePaths().length !== 1;
  startWatchButton.disabled = !watchEventsReady;
  confirmSyncButton.disabled = watchLocked;
  dashboardProfile.disabled = watchLocked || dashboardProfile.options.length <= 1;
  destinationPath.disabled = watchLocked;
  for (const button of directionButtons) button.disabled = watchLocked;
  for (const input of document.querySelectorAll(".exclusion-options input")) {
    input.disabled = watchLocked;
  }
  for (const button of document.querySelectorAll(".selected-source-remove")) {
    button.disabled = watchLocked;
  }
  for (const input of folderBrowserList.querySelectorAll("input")) {
    input.disabled = watchLocked;
  }
  addCurrentFolderButton.disabled = watchLocked || !folderBrowserCurrent;
  addFolderSelectionButton.disabled = watchLocked || folderBrowserSelection.size === 0;
  document.querySelector(".exclusion-count").textContent = String(selectedExclusions().length);
}

function showTransferStatus(message, state = "idle") {
  transferStatus.textContent = message;
  transferStatus.dataset.state = state;
}

function showTransferError(message) {
  showTransferStatus(message, "error");
  transferOutput.textContent = message;
  transferOutput.hidden = false;
}

async function notifyTransfer(title, body) {
  const notifications = desktopApi?.notification;
  if (!notifications) return;

  try {
    let permissionGranted = await notifications.isPermissionGranted();
    if (!permissionGranted) {
      permissionGranted = (await notifications.requestPermission()) === "granted";
    }
    if (!permissionGranted) {
      transferStatus.textContent += " Desktop notifications are not permitted.";
      return;
    }
    await notifications.sendNotification({ title, body });
  } catch (error) {
    transferStatus.textContent += ` Desktop notification failed: ${String(error)}`;
  }
}

function showAgentIntegrationStatus(message, state = "success") {
  agentIntegrationStatus.textContent = message;
  agentIntegrationStatus.dataset.state = state;
  agentIntegrationStatus.hidden = false;
}

function renderAgentActivity(events) {
  const signature = events.map((event) => event.id).join("|");
  if (signature === latestAgentActivitySignature) return;
  latestAgentActivitySignature = signature;
  agentEventList.replaceChildren();

  if (events.length === 0) {
    const empty = document.createElement("div");
    empty.className = "agent-event-empty";
    empty.textContent = "Install the MCP connection or Copilot hooks to see agent activity here.";
    agentEventList.append(empty);
    agentLiveState.textContent = "Waiting for Copilot";
    agentLiveState.dataset.state = "idle";
    return;
  }

  const mostRecentSessionEvent = events.find((event) =>
    event.eventType === "session.started" || event.eventType === "session.ended");
  const sessionActive = mostRecentSessionEvent?.eventType === "session.started";
  agentLiveState.textContent = sessionActive ? "Copilot session active" : "Activity received";
  agentLiveState.dataset.state = sessionActive ? "active" : "idle";

  for (const event of events) {
    const row = document.createElement("article");
    row.className = "agent-event-row";
    row.dataset.type = event.eventType;
    const dot = document.createElement("span");
    dot.className = "agent-event-dot";
    dot.setAttribute("aria-hidden", "true");
    const content = document.createElement("div");
    content.className = "agent-event-summary";
    content.textContent = event.summary;
    const meta = document.createElement("div");
    meta.className = "agent-event-meta";
    meta.textContent = [
      event.source === "mcp" ? "MCP progress report" : "Copilot hook",
      event.toolName ? `Tool: ${event.toolName}` : "",
      event.sessionId ? `Session: ${event.sessionId}` : "",
    ].filter(Boolean).join(" · ");
    content.append(meta);
    const time = document.createElement("time");
    time.className = "agent-event-time";
    time.dateTime = new Date(Number(event.createdAtMillis)).toISOString();
    time.textContent = new Date(Number(event.createdAtMillis)).toLocaleTimeString();
    row.append(dot, content, time);
    agentEventList.append(row);
  }
}

async function loadAgentActivity() {
  if (!desktopApi?.core?.invoke) return;
  try {
    const events = await desktopApi.core.invoke("list_agent_activity");
    renderAgentActivity(events);
  } catch (error) {
    agentLiveState.textContent = `Could not load activity: ${String(error)}`;
    agentLiveState.dataset.state = "idle";
  }
}

installAgentMcpButton.addEventListener("click", async () => {
  if (!desktopApi?.core?.invoke) {
    showAgentIntegrationStatus("MCP setup is available from the Rsync Desktop app.", "error");
    return;
  }
  installAgentMcpButton.disabled = true;
  try {
    const client = agentClientSelect.value;
    let workspacePath;
    if (client === "vscode" || client === "claude-code") {
      if (!desktopApi.dialog?.open) {
        showAgentIntegrationStatus("Workspace selection is available from the Rsync Desktop app.", "error");
        return;
      }
      workspacePath = await desktopApi.dialog.open({
        title: `Choose the ${client === "vscode" ? "VS Code" : "Claude Code"} workspace`,
        directory: true,
        multiple: false,
      });
      if (typeof workspacePath !== "string") return;
    }
    const message = await desktopApi.core.invoke("install_agent_mcp", {
      input: { client, workspacePath },
    });
    showAgentIntegrationStatus(message);
  } catch (error) {
    showAgentIntegrationStatus(`Could not configure the MCP server: ${String(error)}`, "error");
  } finally {
    installAgentMcpButton.disabled = false;
  }
});

agentClientSelect.addEventListener("change", () => {
  const help = {
    vscode: "Adds the local server to the selected VS Code workspace's .vscode/mcp.json.",
    "claude-code": "Adds the local server to the selected Claude Code workspace's .mcp.json.",
    "claude-desktop": "Merges the local server into Claude Desktop's user MCP configuration.",
    "gemini-cli": "Merges the local server into Gemini CLI's ~/.gemini/settings.json.",
    other: "For another MCP-compatible local client, use this stdio command and environment configuration in its MCP server settings.",
  };
  agentClientHelp.textContent = help[agentClientSelect.value];
  const isManual = agentClientSelect.value === "other";
  installAgentMcpButton.hidden = isManual;
  showAgentMcpConfigButton.hidden = !isManual;
  agentMcpConfig.hidden = true;
});

showAgentMcpConfigButton.addEventListener("click", async () => {
  if (!desktopApi?.core?.invoke) {
    showAgentIntegrationStatus("MCP connection details are available from the Rsync Desktop app.", "error");
    return;
  }
  showAgentMcpConfigButton.disabled = true;
  try {
    const config = await desktopApi.core.invoke("get_agent_mcp_config");
    agentMcpConfig.textContent = JSON.stringify({ mcpServers: { "rsync-desktop": config } }, null, 2);
    agentMcpConfig.hidden = false;
  } catch (error) {
    showAgentIntegrationStatus(`Could not get MCP setup details: ${String(error)}`, "error");
  } finally {
    showAgentMcpConfigButton.disabled = false;
  }
});

installAgentHooksButton.addEventListener("click", async () => {
  if (!desktopApi?.dialog?.confirm || !desktopApi?.core?.invoke) {
    showAgentIntegrationStatus("Copilot hook setup is available from the Rsync Desktop app.", "error");
    return;
  }
  const confirmed = await desktopApi.dialog.confirm(
    "Install user-level VS Code Copilot hooks? They run for Copilot agent sessions across workspaces and record only event types and tool names, not prompt text, tool arguments, results, or reasoning. You can remove the generated rsync-desktop.json from ~/.copilot/hooks to uninstall.",
    { title: "Install Copilot activity hooks", kind: "warning" },
  );
  if (!confirmed) return;
  installAgentHooksButton.disabled = true;
  try {
    const message = await desktopApi.core.invoke("install_copilot_hooks");
    showAgentIntegrationStatus(message);
  } catch (error) {
    showAgentIntegrationStatus(`Could not install Copilot hooks: ${String(error)}`, "error");
  } finally {
    installAgentHooksButton.disabled = false;
  }
});

function currentTransferInput() {
  return {
    profileId: dashboardProfile.value,
    direction: activeDirection,
    sources: sourcePaths(),
    destination: destinationPath.value.trim(),
    excludes: selectedExclusions(),
  };
}

function appendPaths(paths) {
  const existing = sourcePaths();
  const next = [...new Set([...existing, ...paths].filter(Boolean))];
  sourceTextarea.value = next.join("\n");
  invalidatePreview();
  renderSelectedSources();
  updateTransferControls();
}

function renderSelectedSources() {
  selectedSourceList.replaceChildren();
  for (const path of sourcePaths()) {
    const item = document.createElement("div");
    item.className = "selected-source-item";
    const name = document.createElement("span");
    name.textContent = path;
    name.title = path;
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "selected-source-remove";
    remove.setAttribute("aria-label", `Remove ${path}`);
    remove.textContent = "×";
    remove.addEventListener("click", () => {
      sourceTextarea.value = sourcePaths().filter((source) => source !== path).join("\n");
      invalidatePreview();
      renderSelectedSources();
      updateTransferControls();
    });
    item.append(name, remove);
    selectedSourceList.append(item);
  }
  selectedSourceList.hidden = !directions[activeDirection].localSources || sourcePaths().length === 0;
}

async function chooseLocalPaths(options) {
  if (!desktopApi?.dialog?.open) {
    showTransferError("File and folder selection is available from the Rsync Desktop app.");
    return;
  }

  try {
    const result = await desktopApi.dialog.open(options);
    if (Array.isArray(result)) appendPaths(result);
    else if (typeof result === "string") appendPaths([result]);
  } catch (error) {
    showTransferError(`Could not select paths: ${String(error)}`);
  }
}

browseFilesButton.addEventListener("click", () => {
  chooseLocalPaths({
    title: "Choose files to transfer",
    multiple: true,
    directory: false,
  });
});

browseFolderButton.addEventListener("click", () => {
  chooseFolderToBrowse();
});

async function chooseFolderToBrowse() {
  if (!desktopApi?.dialog?.open || !desktopApi?.core?.invoke) {
    showTransferError("Folder browsing is available from the Rsync Desktop app.");
    return;
  }

  try {
    const result = await desktopApi.dialog.open({
      title: "Choose a folder to browse",
      multiple: false,
      directory: true,
    });
    if (typeof result !== "string") return;
    folderBrowserRoot = result;
    folderBrowserCurrent = result;
    folderBrowserSelection.clear();
    updateFolderBrowserSelection();
    await loadFolderBrowserDirectory();
  } catch (error) {
    showTransferError(`Could not browse this folder: ${String(error)}`);
  }
}

function updateFolderBrowserSelection() {
  const count = folderBrowserSelection.size;
  folderSelectionCount.textContent = `${count} selected`;
  addFolderSelectionButton.disabled = count === 0;
}

async function loadFolderBrowserDirectory() {
  folderBrowserStatus.textContent = "Loading folder contents…";
  folderBrowserStatus.dataset.state = "pending";
  folderBrowserList.replaceChildren();
  folderBrowserPath.textContent = folderBrowserCurrent;
  folderBrowserUpButton.disabled = folderBrowserCurrent === folderBrowserRoot;
  addCurrentFolderButton.disabled =
    folderBrowserSelection.has(folderBrowserCurrent) || sourcePaths().includes(folderBrowserCurrent);

  try {
    const entries = await desktopApi.core.invoke("list_local_directory", {
      rootPath: folderBrowserRoot,
      path: folderBrowserCurrent,
    });
    folderBrowserList.replaceChildren();
    if (entries.length === 0) {
      const empty = document.createElement("p");
      empty.className = "folder-browser-empty";
      empty.textContent = "This folder is empty.";
      folderBrowserList.append(empty);
    }
    for (const entry of entries) {
      const row = document.createElement("div");
      row.className = "folder-browser-entry";
      const checkbox = document.createElement("input");
      checkbox.type = "checkbox";
      checkbox.checked = folderBrowserSelection.has(entry.path);
      checkbox.setAttribute("aria-label", `Select ${entry.name}`);
      checkbox.addEventListener("change", () => {
        if (checkbox.checked) folderBrowserSelection.set(entry.path, entry);
        else folderBrowserSelection.delete(entry.path);
        updateFolderBrowserSelection();
      });

      const name = document.createElement(entry.isDirectory ? "button" : "span");
      name.className = "folder-browser-entry-name";
      name.textContent = entry.name;
      if (entry.isDirectory) {
        name.type = "button";
        name.addEventListener("click", async () => {
          folderBrowserCurrent = entry.path;
          await loadFolderBrowserDirectory();
        });
      }

      const kind = document.createElement("span");
      kind.className = "folder-browser-entry-kind";
      kind.textContent = entry.isDirectory ? "Folder" : "File";
      row.append(checkbox, name, kind);
      folderBrowserList.append(row);
    }
    folderBrowserStatus.textContent = `${entries.length} item${entries.length === 1 ? "" : "s"}`;
    folderBrowserStatus.dataset.state = "success";
  } catch (error) {
    folderBrowserStatus.textContent = `Could not list folder contents: ${String(error)}`;
    folderBrowserStatus.dataset.state = "error";
  }
}

folderBrowserUpButton.addEventListener("click", async () => {
  if (folderBrowserCurrent === folderBrowserRoot) return;
  const separator = Math.max(folderBrowserCurrent.lastIndexOf("/"), folderBrowserCurrent.lastIndexOf("\\"));
  let parent = folderBrowserCurrent.slice(0, separator || 1);
  if (/^[A-Za-z]:$/.test(parent)) parent += folderBrowserCurrent[separator];
  folderBrowserCurrent = parent || folderBrowserRoot;
  await loadFolderBrowserDirectory();
});

addCurrentFolderButton.addEventListener("click", () => {
  appendPaths([folderBrowserCurrent]);
  addCurrentFolderButton.disabled = true;
});
addFolderSelectionButton.addEventListener("click", () => {
  appendPaths([...folderBrowserSelection.keys()]);
  folderBrowserSelection.clear();
  updateFolderBrowserSelection();
  loadFolderBrowserDirectory();
});

browseDestinationButton.addEventListener("click", async () => {
  if (activeDirection === "push") {
    if (remoteBrowserPanel.hidden) {
      remoteBrowserPanel.hidden = false;
      remoteBrowserRoot = "~/";
      remoteBrowserCurrent = "~/";
      remoteBrowserPathInput.value = "~/";
      await loadRemoteDirectory("~/", "~/");
    } else {
      remoteBrowserPanel.hidden = true;
    }
    return;
  }

  if (!desktopApi?.dialog?.open) {
    showTransferError("Folder selection is available from the Rsync Desktop app.");
    return;
  }
  try {
    const result = await desktopApi.dialog.open({
      title: "Choose a local destination folder",
      multiple: false,
      directory: true,
    });
    if (typeof result === "string") {
      destinationPath.value = result;
      invalidatePreview();
      updateTransferControls();
    }
  } catch (error) {
    showTransferError(`Could not select destination folder: ${String(error)}`);
  }
});

async function loadRemoteDirectory(rootPath, path) {
  if (!dashboardProfile.value || !desktopApi?.core?.invoke) {
    remoteBrowserStatus.textContent = "Choose a server profile to browse remote folders.";
    remoteBrowserStatus.dataset.state = "error";
    return;
  }

  remoteBrowserStatus.textContent = "Loading remote folder contents…";
  remoteBrowserStatus.dataset.state = "pending";
  remoteBrowserList.replaceChildren();
  remoteBrowserSelectButton.disabled = true;
  remoteBrowserUpButton.disabled = true;

  try {
    const listing = await desktopApi.core.invoke("list_remote_directory", {
      profileId: dashboardProfile.value,
      rootPath,
      path,
    });
    remoteBrowserRoot = remoteBrowserRoot || listing.path;
    remoteBrowserCurrent = listing.path;
    remoteBrowserPathInput.value = listing.path;
    remoteBrowserUpButton.disabled = remoteBrowserCurrent === remoteBrowserRoot;
    remoteBrowserSelectButton.disabled = false;

    if (listing.entries.length === 0) {
      const empty = document.createElement("p");
      empty.className = "folder-browser-empty";
      empty.textContent = "This remote folder is empty.";
      remoteBrowserList.append(empty);
    }
    for (const entry of listing.entries) {
      const row = document.createElement("div");
      row.className = "folder-browser-entry";
      const name = document.createElement(entry.isDirectory ? "button" : "span");
      name.className = "folder-browser-entry-name";
      name.textContent = entry.name;
      if (entry.isDirectory) {
        name.type = "button";
        name.addEventListener("click", () => loadRemoteDirectory(remoteBrowserRoot, entry.path));
      }

      const kind = document.createElement("span");
      kind.className = "folder-browser-entry-kind";
      kind.textContent = entry.isDirectory ? "Folder" : "File";
      row.append(name, kind);
      remoteBrowserList.append(row);
    }
    remoteBrowserStatus.textContent =
      `${listing.entries.length} item${listing.entries.length === 1 ? "" : "s"}`;
    remoteBrowserStatus.dataset.state = "success";
  } catch (error) {
    remoteBrowserStatus.textContent = `Could not list remote folder contents: ${String(error)}`;
    remoteBrowserStatus.dataset.state = "error";
  }
}

remoteBrowserOpenButton.addEventListener("click", () => {
  const path = remoteBrowserPathInput.value.trim();
  if (!path) {
    remoteBrowserStatus.textContent = "Enter a remote folder path.";
    remoteBrowserStatus.dataset.state = "error";
    return;
  }
  remoteBrowserRoot = "";
  loadRemoteDirectory(path, path);
});

remoteBrowserPathInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter") {
    event.preventDefault();
    remoteBrowserOpenButton.click();
  }
});

remoteBrowserUpButton.addEventListener("click", () => {
  if (!remoteBrowserCurrent || remoteBrowserCurrent === remoteBrowserRoot) return;
  const separator = remoteBrowserCurrent.lastIndexOf("/");
  const parent = separator <= 0 ? "/" : remoteBrowserCurrent.slice(0, separator);
  loadRemoteDirectory(remoteBrowserRoot, parent);
});

remoteBrowserSelectButton.addEventListener("click", () => {
  destinationPath.value = remoteBrowserCurrent;
  remoteBrowserPanel.hidden = true;
  invalidatePreview();
  updateTransferControls();
});

for (const input of [sourceTextarea, destinationPath]) {
  input.addEventListener("input", () => {
    invalidatePreview();
    renderSelectedSources();
    updateTransferControls();
  });
}

for (const checkbox of document.querySelectorAll(".exclusion-options input")) {
  checkbox.addEventListener("change", () => {
    invalidatePreview();
    updateTransferControls();
  });
}

previewButton.addEventListener("click", async () => {
  if (!desktopApi?.core?.invoke) {
    showTransferError("Transfer preview is available from the Rsync Desktop app.");
    return;
  }

  invalidatePreview();
  previewButton.disabled = true;
  transferOutput.hidden = true;
  showTransferStatus("Creating a dry-run preview…", "pending");
  try {
    currentPreviewInput = currentTransferInput();
    currentPreview = await desktopApi.core.invoke("preview_rsync", {
      input: currentPreviewInput,
    });
    currentTransferId = dashboardProfile.value;
    renderTransferPreview(currentPreview.output);
    previewSummary.textContent =
      `Previewed ${currentPreview.sourceCount} source item${currentPreview.sourceCount === 1 ? "" : "s"}. This is a dry run; no files were transferred.`;
    populateCompareSources(currentPreviewInput.sources);
    currentFileComparison = null;
    fileDiffOutput.replaceChildren();
    fileDiffOutput.hidden = true;
    fileCompareStatus.textContent = "Choose a file to compare its local and remote contents.";
    fileCompareStatus.dataset.state = "idle";
    confirmSyncButton.disabled = false;
    transferDialog.showModal();
    showTransferStatus("Preview ready. Review the changes before starting a live sync.", "success");
  } catch (error) {
    showTransferError(String(error));
  } finally {
    updateTransferControls();
  }
});

function renderTransferPreview(output) {
  previewOutput.replaceChildren();
  const lines = String(output || "").split(/\r?\n/).filter((line) => line.trim());
  if (lines.length === 0) {
    const empty = document.createElement("p");
    empty.className = "preview-tree-empty";
    empty.textContent = "No file changes detected.";
    previewOutput.append(empty);
    return;
  }

  const counts = { added: 0, updated: 0, deleted: 0, other: 0 };
  for (const rawLine of lines) {
    const separator = rawLine.indexOf(" ");
    const code = separator < 0 ? "" : rawLine.slice(0, separator);
    const path = separator < 0 ? rawLine : rawLine.slice(separator + 1);
    let change = "other";
    let label = "Details";
    if (code === "*deleting") {
      change = "deleted";
      label = "Delete";
    } else if (code.includes("+++++++++")) {
      change = "added";
      label = "Add";
    } else if (code.startsWith(">") || code.startsWith("<") || code.startsWith("c")) {
      change = "updated";
      label = "Update";
    }
    counts[change] += 1;

    const row = document.createElement("div");
    row.className = `preview-tree-row preview-tree-${change}`;
    row.setAttribute("role", "listitem");
    const badge = document.createElement("span");
    badge.className = "preview-tree-badge";
    badge.textContent = label;
    const itemPath = document.createElement("span");
    itemPath.className = "preview-tree-path";
    itemPath.textContent = path;
    itemPath.title = path;
    row.append(badge, itemPath);
    previewOutput.append(row);
  }

  const summary = document.createElement("p");
  summary.className = "preview-tree-counts";
  summary.textContent = [
    `${counts.added} added`,
    `${counts.updated} updated`,
    `${counts.deleted} deleted`,
  ].join(" · ");
  previewOutput.prepend(summary);
}

function populateCompareSources(sources) {
  compareSourceSelect.replaceChildren();
  for (const source of sources) {
    const option = document.createElement("option");
    option.value = source;
    option.textContent = source;
    option.title = source;
    compareSourceSelect.append(option);
  }
  compareFileButton.disabled = sources.length === 0;
}

function diffOperations(leftText, rightText) {
  const leftLines = (leftText ?? "").replace(/\r\n/g, "\n").split("\n");
  const rightLines = (rightText ?? "").replace(/\r\n/g, "\n").split("\n");
  const leftCount = leftLines.length;
  const rightCount = rightLines.length;
  if ((leftCount + 1) * (rightCount + 1) > 2_000_000) {
    throw new Error("This file has too many lines for an in-app diff.");
  }

  const width = rightCount + 1;
  const table = new Uint16Array((leftCount + 1) * width);
  for (let left = leftCount - 1; left >= 0; left -= 1) {
    for (let right = rightCount - 1; right >= 0; right -= 1) {
      const index = left * width + right;
      table[index] = leftLines[left] === rightLines[right]
        ? table[(left + 1) * width + right + 1] + 1
        : Math.max(table[(left + 1) * width + right], table[index + 1]);
    }
  }

  const operations = [];
  let left = 0;
  let right = 0;
  while (left < leftCount && right < rightCount) {
    if (leftLines[left] === rightLines[right]) {
      operations.push({ type: "context", left: leftLines[left], right: rightLines[right] });
      left += 1;
      right += 1;
    } else if (table[left * width + right + 1] > table[(left + 1) * width + right]) {
      operations.push({ type: "add", right: rightLines[right] });
      right += 1;
    } else {
      operations.push({ type: "remove", left: leftLines[left] });
      left += 1;
    }
  }
  while (left < leftCount) {
    operations.push({ type: "remove", left: leftLines[left] });
    left += 1;
  }
  while (right < rightCount) {
    operations.push({ type: "add", right: rightLines[right] });
    right += 1;
  }
  return operations;
}

function createDiffLine(text, number, type) {
  const line = document.createElement("div");
  line.className = `diff-line diff-line-${type}`;
  const lineNumber = document.createElement("span");
  lineNumber.className = "diff-line-number";
  lineNumber.textContent = number === null ? "" : String(number);
  const content = document.createElement("span");
  content.className = "diff-line-content";
  content.textContent = text ?? "";
  line.append(lineNumber, content);
  return line;
}

function renderFileComparison() {
  if (!currentFileComparison) return;
  const { localContent, remoteContent, localPath, remotePath } = currentFileComparison;
  const operations = diffOperations(localContent, remoteContent);
  fileDiffOutput.replaceChildren();
  fileDiffOutput.hidden = false;

  const headings = document.createElement("div");
  headings.className = `diff-headings diff-headings-${diffFormatSelect.value}`;
  for (const [label, path] of [["Local", localPath], ["Remote", remotePath]]) {
    const heading = document.createElement("div");
    heading.className = "diff-file-heading";
    const side = document.createElement("strong");
    side.textContent = label;
    const file = document.createElement("span");
    file.textContent = path;
    file.title = path;
    heading.append(side, file);
    headings.append(heading);
  }
  fileDiffOutput.append(headings);

  if (localContent === remoteContent) {
    const identical = document.createElement("p");
    identical.className = "diff-identical";
    identical.textContent = "Files are identical.";
    fileDiffOutput.append(identical);
    fileCompareStatus.textContent = "Files are identical.";
    fileCompareStatus.dataset.state = "success";
    return;
  }

  if (diffFormatSelect.value === "unified") {
    const unified = document.createElement("div");
    unified.className = "diff-unified";
    let leftLine = 1;
    let rightLine = 1;
    for (const operation of operations) {
      const text = operation.type === "context"
        ? ` ${operation.left}`
        : operation.type === "add"
          ? `+${operation.right}`
          : `-${operation.left}`;
      const line = createDiffLine(text, operation.type === "add" ? rightLine : leftLine, operation.type);
      unified.append(line);
      if (operation.type !== "add") leftLine += 1;
      if (operation.type !== "remove") rightLine += 1;
    }
    fileDiffOutput.append(unified);
  } else {
    const split = document.createElement("div");
    split.className = "diff-split";
    const leftColumn = document.createElement("div");
    const rightColumn = document.createElement("div");
    let leftLine = 1;
    let rightLine = 1;
    for (let index = 0; index < operations.length;) {
      const operation = operations[index];
      if (operation.type === "context") {
        leftColumn.append(createDiffLine(operation.left, leftLine++, "context"));
        rightColumn.append(createDiffLine(operation.right, rightLine++, "context"));
        index += 1;
        continue;
      }

      const removed = [];
      const added = [];
      while (index < operations.length && operations[index].type !== "context") {
        if (operations[index].type === "remove") removed.push(operations[index].left);
        else added.push(operations[index].right);
        index += 1;
      }
      const changedRows = Math.max(removed.length, added.length);
      for (let row = 0; row < changedRows; row += 1) {
        leftColumn.append(createDiffLine(removed[row] ?? "", removed[row] === undefined ? null : leftLine++, removed[row] === undefined ? "empty" : "remove"));
        rightColumn.append(createDiffLine(added[row] ?? "", added[row] === undefined ? null : rightLine++, added[row] === undefined ? "empty" : "add"));
      }
    }
    split.append(leftColumn, rightColumn);
    fileDiffOutput.append(split);
  }

  fileDiffOutput.hidden = false;
  fileCompareStatus.textContent =
    `${localContent === null ? "Local file does not exist yet. " : ""}${remoteContent === null ? "Remote file does not exist yet." : "Differences are highlighted."}`.trim();
  fileCompareStatus.dataset.state = "success";
}

compareFileButton.addEventListener("click", async () => {
  if (!currentPreviewInput || !currentPreview || !desktopApi?.core?.invoke) return;
  compareFileButton.disabled = true;
  fileDiffOutput.hidden = true;
  fileCompareStatus.textContent = "Comparing file contents…";
  fileCompareStatus.dataset.state = "pending";
  try {
    currentFileComparison = await desktopApi.core.invoke("compare_transfer_file", {
      input: {
        profileId: currentPreviewInput.profileId,
        direction: currentPreviewInput.direction,
        source: compareSourceSelect.value,
        destination: currentPreviewInput.destination,
      },
    });
    renderFileComparison();
  } catch (error) {
    currentFileComparison = null;
    fileCompareStatus.textContent = String(error);
    fileCompareStatus.dataset.state = "error";
  } finally {
    compareFileButton.disabled = !currentPreviewInput?.sources.length;
  }
});

diffFormatSelect.addEventListener("change", () => {
  if (currentFileComparison) renderFileComparison();
});

function closeTransferDialog() {
  transferDialog.close();
}

startSyncButton.addEventListener("click", () => {
  if (!currentPreview) return;
  transferDialog.showModal();
});

document.querySelector("#close-transfer-dialog").addEventListener("click", closeTransferDialog);
document.querySelector("#cancel-transfer-button").addEventListener("click", closeTransferDialog);
transferDialog.addEventListener("click", (event) => {
  if (event.target === transferDialog) closeTransferDialog();
});

startWatchButton.addEventListener("click", async () => {
  if (!currentPreview || !currentPreviewInput || !desktopApi?.core?.invoke) return;
  if (!watchEventsReady) {
    showTransferError("Watch & Sync status updates are unavailable. Restart the app and try again.");
    return;
  }

  const confirmed = await desktopApi.dialog.confirm(
    "Start by syncing this approved preview, then automatically push changes from this one local folder to the selected server. Protected files remain excluded and remote-only files are never deleted. Keep the app open, and stop watching when you are done.",
    { title: "Start Watch & Sync", kind: "warning" },
  );
  if (!confirmed) return;

  const previewId = currentPreview.id;
  pendingWatcherStart = true;
  pendingWatchStatus = null;
  updateTransferControls();
  showTransferStatus("Starting the approved sync and folder watcher…", "pending");
  try {
    const result = await desktopApi.core.invoke("start_directory_watcher", { previewId });
    currentPreview = null;
    activeWatcherId = result.watcherId;
    pendingWatcherStart = false;
    closeTransferDialog();
    transferOutput.textContent = result.output || "Initial sync completed.";
    transferOutput.hidden = false;
    showTransferStatus("Watch & Sync is active. Changes will be pushed automatically.", "success");
    const latestStatus = pendingWatchStatus;
    pendingWatchStatus = null;
    if (latestStatus?.watcherId === activeWatcherId) {
      showWatchStatus(latestStatus);
    } else {
      showWatchStatus({
        watcherId: activeWatcherId,
        state: "watching",
        message: "Initial sync completed. Watching this folder for changes.",
      });
    }
    await notifyTransfer(
      "Rsync Desktop: Watch & Sync started",
      "The initial sync completed. Changes in the selected folder will be pushed automatically.",
    );
  } catch (error) {
    currentPreview = null;
    const latestStatus = pendingWatchStatus;
    pendingWatcherStart = false;
    pendingWatchStatus = null;
    closeTransferDialog();
    showTransferError(String(error));
    if (latestStatus?.state === "stopped") {
      watchSessionPanel.hidden = false;
      watchSessionStatus.textContent = latestStatus.message;
      watchSessionStatus.dataset.state = "error";
    }
  } finally {
    pendingWatcherStart = false;
    updateTransferControls();
  }
});

stopWatchButton.addEventListener("click", async () => {
  if (!activeWatcherId || !desktopApi?.core?.invoke) return;
  const watcherId = activeWatcherId;
  stopWatchButton.disabled = true;
  watchSessionStatus.textContent = "Stopping after any current sync finishes…";
  watchSessionStatus.dataset.state = "pending";
  try {
    await desktopApi.core.invoke("stop_directory_watcher", { watcherId });
  } catch (error) {
    stopWatchButton.disabled = false;
    watchSessionStatus.textContent = `Could not stop Watch & Sync: ${String(error)}`;
    watchSessionStatus.dataset.state = "error";
  }
});

confirmSyncButton.addEventListener("click", async () => {
  if (!currentPreview || !desktopApi?.core?.invoke) return;
  if (currentTransferId !== dashboardProfile.value) {
    showTransferError("The selected server profile changed. Create a new preview before syncing.");
    closeTransferDialog();
    invalidatePreview();
    return;
  }

  confirmSyncButton.disabled = true;
  showTransferStatus("Syncing… Keep the app open until the transfer finishes.", "pending");
  try {
    const result = await desktopApi.core.invoke("execute_rsync", {
      previewId: currentPreview.id,
    });
    currentPreview = null;
    closeTransferDialog();
    transferOutput.textContent = result.output || "Transfer completed.";
    transferOutput.hidden = false;
    showTransferStatus(
      `Transfer completed: ${result.sourceCount} source item${result.sourceCount === 1 ? "" : "s"}.`,
      "success",
    );
    await notifyTransfer(
      "Rsync Desktop: transfer complete",
      `${result.sourceCount} source item${result.sourceCount === 1 ? "" : "s"} synced successfully.`,
    );
  } catch (error) {
    currentPreview = null;
    closeTransferDialog();
    showTransferError(String(error));
    await notifyTransfer("Rsync Desktop: transfer failed", String(error));
  } finally {
    confirmSyncButton.disabled = false;
    updateTransferControls();
  }
});

function showCurrentView() {
  const requestedView = window.location.hash.slice(1);
  const activeView = Object.hasOwn(viewTitles, requestedView) ? requestedView : "dashboard";

  for (const view of views) {
    view.hidden = view.dataset.view !== activeView;
  }

  for (const link of navigationLinks) {
    const isActive = link.hash === `#${activeView}`;
    link.classList.toggle("is-active", isActive);
    if (isActive) {
      link.setAttribute("aria-current", "page");
    } else {
      link.removeAttribute("aria-current");
    }
  }

  const separator = document.createElement("span");
  separator.textContent = "/";
  breadcrumb.replaceChildren("Workspace ", separator, ` ${viewTitles[activeView]}`);
  document.title = `${viewTitles[activeView]} | Rsync Desktop`;
  if (activeView === "agent-activity") loadAgentActivity();
}

window.addEventListener("hashchange", showCurrentView);
window.setInterval(() => {
  if (window.location.hash === "#agent-activity") loadAgentActivity();
}, 1500);
for (const link of viewLinks) {
  link.addEventListener("click", (event) => {
    const requestedView = link.hash.slice(1);
    if (!Object.hasOwn(viewTitles, requestedView)) return;

    event.preventDefault();
    window.location.hash = requestedView;
    showCurrentView();
  });
}
showCurrentView();

function showProfileMessage(message, state = "error") {
  profileMessage.textContent = message;
  profileMessage.dataset.state = state;
  profileMessage.hidden = false;
}

function clearProfileMessage() {
  profileMessage.textContent = "";
  delete profileMessage.dataset.state;
  profileMessage.hidden = true;
}

function showFormError(message) {
  profileFormError.textContent = message;
  profileFormError.hidden = false;
}

function clearFormError() {
  profileFormError.textContent = "";
  profileFormError.hidden = true;
}

function renderProfiles(profiles) {
  profileProfiles.splice(0, profileProfiles.length, ...profiles);
  profileList.replaceChildren();
  profileTotal.textContent = String(profiles.length);
  profileNavCount.textContent = String(profiles.length);
  profileEmpty.hidden = profiles.length > 0;

  const selectedProfileId = dashboardProfile.value;
  dashboardProfile.replaceChildren();
  if (profiles.length === 0) {
    const option = document.createElement("option");
    option.value = "";
    option.textContent = "No server profiles yet";
    option.selected = true;
    dashboardProfile.append(option);
    dashboardProfile.disabled = true;
  } else {
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = "Select a server profile";
    dashboardProfile.append(placeholder);

    for (const profile of profiles) {
      const option = document.createElement("option");
      option.value = profile.id;
      option.textContent = profile.name;
      dashboardProfile.append(option);

      const card = document.createElement("article");
      card.className = "profile-card";
      card.dataset.profileId = profile.id;

      const avatar = document.createElement("span");
      avatar.className = "profile-card-icon";
      avatar.setAttribute("aria-hidden", "true");
      avatar.textContent = profile.name.slice(0, 1).toUpperCase();

      const details = document.createElement("div");
      details.className = "profile-card-details";
      const title = document.createElement("h3");
      title.textContent = profile.name;
      const address = document.createElement("p");
      address.textContent = `${profile.username}@${profile.host} · Port ${profile.port}`;
      const testStatus = document.createElement("p");
      testStatus.className = "profile-test-status";
      testStatus.dataset.state = "idle";
      testStatus.setAttribute("role", "status");
      testStatus.hidden = true;
      const keyPath = document.createElement("p");
      keyPath.className = "profile-card-key";
      keyPath.textContent = profile.keyPath || "Default SSH key";
      details.append(title, address, keyPath, testStatus);
      const gitRepositories = profile.gitRepositories ?? (
        profile.gitLocalPath && profile.gitRemotePath
          ? [{ branch: profile.gitBranch || "main" }]
          : []
      );
      if (gitRepositories.length > 0) {
        const gitDeployment = document.createElement("p");
        gitDeployment.className = "profile-card-key";
        gitDeployment.textContent = `${gitRepositories.length} Git ${gitRepositories.length === 1 ? "repository" : "repositories"}`;
        details.append(gitDeployment);
      }
      if (profile.hasGithubToken) {
        const tokenStatus = document.createElement("p");
        tokenStatus.className = "profile-card-key";
        tokenStatus.textContent = "GitHub token saved in OS credential store";
        details.append(tokenStatus);
      }

      const actions = document.createElement("div");
      actions.className = "profile-card-actions";
      const testButton = document.createElement("button");
      testButton.type = "button";
      testButton.className = "profile-action";
      testButton.dataset.profileAction = "test";
      testButton.dataset.profileId = profile.id;
      testButton.textContent = "Test SSH";
      actions.append(testButton);
      const terminalButton = document.createElement("button");
      terminalButton.type = "button";
      terminalButton.className = "profile-action";
      terminalButton.dataset.profileAction = "terminal";
      terminalButton.dataset.profileId = profile.id;
      terminalButton.textContent = "SSH Terminal";
      actions.append(terminalButton);
      if (gitRepositories.length > 0) {
        const deployButton = document.createElement("button");
        deployButton.type = "button";
        deployButton.className = "profile-action profile-action-deploy";
        deployButton.dataset.profileAction = "deploy";
        deployButton.dataset.profileId = profile.id;
        deployButton.textContent = gitRepositories.length === 1 ? "Deploy" : "Deploy all";
        actions.append(deployButton);
      }
      for (const [action, label] of [["edit", "Edit"], ["delete", "Delete"]]) {
        const button = document.createElement("button");
        button.type = "button";
        button.className = `profile-action profile-action-${action}`;
        button.dataset.profileAction = action;
        button.dataset.profileId = profile.id;
        button.textContent = label;
        actions.append(button);
      }

      card.append(avatar, details, actions);
      profileList.append(card);
    }

    dashboardProfile.disabled = false;
    dashboardProfile.value = profiles.some((profile) => profile.id === selectedProfileId)
      ? selectedProfileId
      : "";
  }

  const profileSelected = dashboardProfile.value !== "";
  document.querySelector("#profile-note").textContent = profileSelected
    ? "Selected SSH connection."
    : "Choose a saved SSH connection.";
  updateTransferControls();
}

async function loadProfiles() {
  if (!desktopApi?.core?.invoke) {
    showProfileMessage("Server profiles are available from the Rsync Desktop app.");
    return;
  }

  try {
    const profiles = await desktopApi.core.invoke("get_profiles");
    renderProfiles(profiles);
    clearProfileMessage();
  } catch (error) {
    showProfileMessage(`Could not load server profiles: ${String(error)}`);
  }
}

function openProfileDialog(profile = null) {
  clearFormError();
  profileForm.reset();
  profileForm.elements.id.value = profile?.id ?? "";
  profileForm.elements.name.value = profile?.name ?? "";
  profileForm.elements.host.value = profile?.host ?? "";
  profileForm.elements.port.value = String(profile?.port ?? 22);
  profileForm.elements.username.value = profile?.username ?? "";
  profileForm.elements.keyPath.value = profile?.keyPath ?? "";
  profileForm.elements.githubToken.value = "";
  document.querySelector("#github-token-status").textContent = profile?.hasGithubToken
    ? "A token is saved in the OS credential store. Enter a new value to replace it."
    : "Use a classic token with the repo scope for private repositories. Tokens are stored in the OS credential store and are only used for local pushes to https://github.com.";
  document.querySelector("#remove-github-token-button").disabled = !profile?.hasGithubToken;
  gitRepositoryList.replaceChildren();
  const gitRepositories = profile?.gitRepositories ?? (
    profile?.gitLocalPath && profile?.gitRemotePath
      ? [{ localPath: profile.gitLocalPath, remotePath: profile.gitRemotePath, branch: profile.gitBranch || "main" }]
      : []
  );
  gitRepositories.forEach((repository) => addGitRepositoryRow(repository));
  document.querySelector("#profile-dialog-title").textContent = profile
    ? "Edit server profile"
    : "Add server profile";
  profileSaveButton.textContent = profile ? "Save changes" : "Save profile";
  profileDialog.showModal();
  profileForm.elements.name.focus();
}

document.querySelector("#add-profile-button").addEventListener("click", () => {
  openProfileDialog();
});

for (const button of document.querySelectorAll("[data-close-dialog]")) {
  button.addEventListener("click", () => profileDialog.close());
}

profileDialog.addEventListener("click", (event) => {
  if (event.target === profileDialog) profileDialog.close();
});

profileList.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-profile-action]");
  if (!button) return;

  const profile = profileProfiles.find((item) => item.id === button.dataset.profileId);
  if (!profile) {
    showProfileMessage("That server profile is no longer available. Reload the view and try again.");
    return;
  }

  clearProfileMessage();
  if (button.dataset.profileAction === "edit") {
    openProfileDialog(profile);
    return;
  }

  if (button.dataset.profileAction === "terminal" && desktopApi?.core?.invoke) {
    try {
      await desktopApi.core.invoke("open_ssh_terminal", { profileId: profile.id });
      showProfileMessage(`Opened an SSH terminal for "${profile.name}".`, "success");
    } catch (error) {
      showProfileMessage(`Could not open SSH terminal: ${String(error)}`);
    }
    return;
  }

  if (button.dataset.profileAction === "test" && desktopApi?.core?.invoke) {
    const status = button.closest(".profile-card").querySelector(".profile-test-status");
    button.disabled = true;
    status.hidden = false;
    status.textContent = "Testing SSH connection…";
    status.dataset.state = "pending";
    try {
      status.textContent = await desktopApi.core.invoke("test_ssh_connection", {
        profileId: profile.id,
      });
      status.dataset.state = "success";
    } catch (error) {
      status.textContent = `SSH test failed: ${String(error)}`;
      status.dataset.state = "error";
    } finally {
      button.disabled = false;
    }
    return;
  }

  if (button.dataset.profileAction === "deploy" && desktopApi?.core?.invoke) {
    try {
      const repositories = profile.gitRepositories ?? [];
      const confirmed = await desktopApi.dialog.confirm(
        `Deploy ${repositories.length} Git ${repositories.length === 1 ? "repository" : "repositories"}? Each will push committed HEAD to origin, then fast-forward its configured server checkout. A saved GitHub token is used only for local pushes to https://github.com; the server checkout must have its own GitHub access configured. The deployment stops if a server checkout has local changes.`,
        { title: "Deploy Git repositories", kind: "warning" },
      );
      if (!confirmed) return;
      button.disabled = true;
      button.textContent = "Deploying…";
      const completed = [];
      for (let index = 0; index < repositories.length; index += 1) {
        try {
          const result = await desktopApi.core.invoke("deploy_git_repository", {
            profileId: profile.id,
            repositoryIndex: index,
          });
          completed.push(`Repository ${index + 1} (${repositories[index].branch}): ${result}`);
        } catch (error) {
          const prior = completed.length
            ? `Completed before failure:\n${completed.join("\n\n")}\n\n`
            : "";
          showProfileMessage(`${prior}Repository ${index + 1} failed: ${String(error)}`);
          return;
        }
      }
      showProfileMessage(completed.join("\n\n"), "success");
    } catch (error) {
      showProfileMessage(`Git deployment failed: ${String(error)}`);
    } finally {
      button.disabled = false;
      button.textContent = repositories.length === 1 ? "Deploy" : "Deploy all";
    }
    return;
  }

  if (button.dataset.profileAction !== "delete" || !desktopApi?.core?.invoke) return;
  try {
    const confirmed = await desktopApi.dialog.confirm(
      `Delete the "${profile.name}" server profile?`,
      { title: "Delete server profile", kind: "warning" },
    );
    if (!confirmed) return;
    await desktopApi.core.invoke("delete_profile", { profileId: profile.id });
    await loadProfiles();
  } catch (error) {
    await loadProfiles();
    showProfileMessage(String(error));
  }
});

document.querySelector("#browse-key-button").addEventListener("click", async () => {
  if (!desktopApi?.dialog?.open) {
    showFormError("The private-key picker is available from the Rsync Desktop app.");
    return;
  }

  try {
    const selectedPath = await desktopApi.dialog.open({
      title: "Choose an SSH private key",
      multiple: false,
      directory: false,
    });
    if (typeof selectedPath === "string") {
      profileForm.elements.keyPath.value = selectedPath;
      clearFormError();
    }
  } catch (error) {
    showFormError(`Could not open the private-key picker: ${String(error)}`);
  }
});

document.querySelector("#remove-github-token-button").addEventListener("click", async () => {
  const profileId = profileForm.elements.id.value;
  if (!profileId || !desktopApi?.core?.invoke) return;
  const button = document.querySelector("#remove-github-token-button");
  button.disabled = true;
  try {
    await desktopApi.core.invoke("clear_github_token", { profileId });
    document.querySelector("#github-token-status").textContent =
      "Saved GitHub token removed from the OS credential store.";
    await loadProfiles();
  } catch (error) {
    showFormError(`Could not remove the GitHub token: ${String(error)}`);
    button.disabled = false;
  }
});

function addGitRepositoryRow(repository = {}) {
  const row = document.createElement("section");
  row.className = "git-repository-row";

  const heading = document.createElement("div");
  heading.className = "git-repository-row-heading";
  const title = document.createElement("strong");
  title.textContent = `Repository ${gitRepositoryList.children.length + 1}`;
  const remove = document.createElement("button");
  remove.type = "button";
  remove.className = "profile-action profile-action-delete";
  remove.dataset.gitRepositoryAction = "remove";
  remove.textContent = "Remove";
  heading.append(title, remove);

  const localLabel = document.createElement("label");
  localLabel.textContent = "Local Git repository";
  const localInputRow = document.createElement("div");
  localInputRow.className = "key-path-input";
  const localInput = document.createElement("input");
  localInput.type = "text";
  localInput.placeholder = "/path/to/local/repository";
  localInput.autocomplete = "off";
  localInput.dataset.gitField = "localPath";
  localInput.value = repository.localPath ?? "";
  const browse = document.createElement("button");
  browse.type = "button";
  browse.className = "button button-secondary";
  browse.dataset.gitRepositoryAction = "browse";
  browse.textContent = "Browse";
  localInputRow.append(localInput, browse);
  localLabel.append(localInputRow);

  const remoteLabel = document.createElement("label");
  remoteLabel.textContent = "Server Git checkout";
  const remoteInput = document.createElement("input");
  remoteInput.type = "text";
  remoteInput.placeholder = "/absolute/server/repository/path";
  remoteInput.autocomplete = "off";
  remoteInput.dataset.gitField = "remotePath";
  remoteInput.value = repository.remotePath ?? "";
  remoteLabel.append(remoteInput);

  const branchLabel = document.createElement("label");
  branchLabel.textContent = "Branch";
  const branchInput = document.createElement("input");
  branchInput.type = "text";
  branchInput.maxLength = 255;
  branchInput.autocomplete = "off";
  branchInput.dataset.gitField = "branch";
  branchInput.value = repository.branch ?? "main";
  branchLabel.append(branchInput);

  row.append(heading, localLabel, remoteLabel, branchLabel);
  gitRepositoryList.append(row);
}

document.querySelector("#add-git-repository-button").addEventListener("click", () => {
  addGitRepositoryRow();
});

gitRepositoryList.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-git-repository-action]");
  if (!button) return;
  const row = button.closest(".git-repository-row");
  if (button.dataset.gitRepositoryAction === "remove") {
    row.remove();
    [...gitRepositoryList.children].forEach((item, index) => {
      item.querySelector(".git-repository-row-heading strong").textContent = `Repository ${index + 1}`;
    });
    return;
  }
  if (!desktopApi?.dialog?.open) {
    showFormError("The Git repository picker is available from the Rsync Desktop app.");
    return;
  }

  try {
    const selectedPath = await desktopApi.dialog.open({
      title: "Choose a local Git repository",
      multiple: false,
      directory: true,
    });
    if (typeof selectedPath === "string") {
      row.querySelector('[data-git-field="localPath"]').value = selectedPath;
      clearFormError();
    }
  } catch (error) {
    showFormError(`Could not open the Git repository picker: ${String(error)}`);
  }
});

profileForm.addEventListener("submit", async (event) => {
  event.preventDefault();
  clearFormError();
  if (!desktopApi?.core?.invoke) {
    showFormError("Server profiles can only be saved from the Rsync Desktop app.");
    return;
  }

  const formData = new FormData(profileForm);
  const profile = {
    id: formData.get("id") || null,
    name: formData.get("name"),
    host: formData.get("host"),
    port: Number(formData.get("port")),
    username: formData.get("username"),
    keyPath: formData.get("keyPath") || null,
    gitRepositories: [...gitRepositoryList.children]
      .map((row) => ({
        localPath: row.querySelector('[data-git-field="localPath"]').value.trim(),
        remotePath: row.querySelector('[data-git-field="remotePath"]').value.trim(),
        branch: row.querySelector('[data-git-field="branch"]').value.trim() || "main",
      }))
      .filter((repository) => repository.localPath || repository.remotePath),
  };

  profileSaveButton.disabled = true;
  profileSaveButton.textContent = "Saving…";
  let profileSaved = false;
  try {
    const savedProfile = await desktopApi.core.invoke("save_profile", { profile });
    profileSaved = true;
    const githubToken = String(formData.get("githubToken") || "").trim();
    if (githubToken) {
      await desktopApi.core.invoke("set_github_token", {
        profileId: savedProfile.id,
        token: githubToken,
      });
      profileForm.elements.githubToken.value = "";
    }
    await loadProfiles();
    profileDialog.close();
  } catch (error) {
    showFormError(
      profileSaved
        ? `Profile saved, but the GitHub token could not be stored: ${String(error)}`
        : String(error),
    );
  } finally {
    profileSaveButton.disabled = false;
    profileSaveButton.textContent = profile.id ? "Save changes" : "Save profile";
  }
});

dashboardProfile.addEventListener("change", () => {
  invalidatePreview();
  remoteBrowserPanel.hidden = true;
  remoteBrowserRoot = "";
  remoteBrowserCurrent = "";
  document.querySelector("#profile-note").textContent = dashboardProfile.value
    ? "Selected SSH connection."
    : "Choose a saved SSH connection.";
  updateTransferControls();
});

setDirection("push");
loadProfiles();
