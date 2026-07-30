// Message Exporters — web GUI
//
// Two small, self-contained pieces of progressive enhancement. Everything
// else (navigation, forms, running jobs) works with plain HTML + a full
// page load; nothing here is required for the app to function.
//   1. Live log streaming on the job page (Server-Sent Events).
//   2. A folder/file browse dialog backed by GET /api/browse.

(function liveLog() {
  const status = document.getElementById("job-status");
  const output = document.getElementById("log-output");
  if (!status || !output) return;

  const url = status.dataset.eventsUrl;
  const source = new EventSource(url);

  function appendLine(text, cssClass) {
    const atBottom = output.scrollTop + output.clientHeight >= output.scrollHeight - 4;
    const line = document.createElement("div");
    if (cssClass) line.className = "log-line " + cssClass;
    line.textContent = text;
    output.appendChild(line);
    if (atBottom) output.scrollTop = output.scrollHeight;
  }

  function setStatus(label, cssClass) {
    status.textContent = label;
    status.className = "badge " + cssClass;
  }

  source.addEventListener("started", (event) => appendLine("$ " + event.data, "log-line-started"));
  source.addEventListener("log", (event) => appendLine(event.data, ""));
  source.addEventListener("finished", (event) => {
    appendLine(event.data, "log-line-finished");
    setStatus("Finished", "badge-done");
    disableCancel();
    source.close();
  });
  source.addEventListener("error-event", (event) => {
    appendLine(event.data, "log-line-error");
    setStatus("Error", "badge-error");
    disableCancel();
    source.close();
  });

  function disableCancel() {
    const cancel = document.querySelector('form[action$="/cancel"] button');
    if (cancel) cancel.disabled = true;
  }
})();

(function browseDialog() {
  const dialog = document.getElementById("browse-dialog");
  if (!dialog) return;

  const pathInput = document.getElementById("browse-path");
  const list = document.getElementById("browse-list");
  const upButton = document.getElementById("browse-up");
  const errorLabel = document.getElementById("browse-error");
  const chooseButton = document.getElementById("browse-choose");
  const cancelButton = document.getElementById("browse-cancel");

  let targetInput = null;
  let current = "";
  let parent = null;

  async function load(path) {
    errorLabel.textContent = "";
    try {
      const response = await fetch("/api/browse?path=" + encodeURIComponent(path || ""));
      if (!response.ok) throw new Error(await response.text());
      const data = await response.json();
      current = data.path;
      parent = data.parent;
      pathInput.value = current;
      upButton.disabled = !parent;
      renderEntries(data.entries);
    } catch (error) {
      errorLabel.textContent = String(error.message || error);
    }
  }

  function renderEntries(entries) {
    list.innerHTML = "";
    for (const entry of entries) {
      const li = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      const icon = document.createElement("span");
      icon.className = "entry-icon";
      icon.textContent = entry.is_dir ? "📁" : "📄";
      const name = document.createElement("span");
      name.textContent = entry.name;
      button.append(icon, name);
      button.addEventListener("click", () => {
        if (entry.is_dir) {
          load(entry.path);
        } else {
          targetInput.value = entry.path;
          dialog.close();
        }
      });
      li.appendChild(button);
      list.appendChild(li);
    }
  }

  document.querySelectorAll(".browse[data-browse-target]").forEach((button) => {
    button.addEventListener("click", () => {
      targetInput = document.getElementById(button.dataset.browseTarget);
      dialog.showModal();
      load(targetInput.value || "");
    });
  });

  upButton.addEventListener("click", () => {
    if (parent !== null) load(parent);
  });
  cancelButton.addEventListener("click", () => dialog.close());
  chooseButton.addEventListener("click", () => {
    targetInput.value = current;
    dialog.close();
  });
})();
