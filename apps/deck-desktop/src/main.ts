import { mount } from "svelte";
import App from "./App.svelte";
import { loadTexts } from "./i18n";

await loadTexts();
const target = document.getElementById("app");
if (target) mount(App, { target });
