const app = document.querySelector<HTMLElement>("#app");

if (!app) {
  throw new Error("Missing #app element");
}

const heading = document.createElement("h1");
heading.textContent = "Herdr Lens";

const description = document.createElement("p");
description.textContent = "The agent conversation viewer will appear here.";

app.append(heading, description);
