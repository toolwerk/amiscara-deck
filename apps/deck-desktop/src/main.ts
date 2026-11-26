import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App.svelte";
import MainWindow from "./MainWindow.svelte";
import { loadTexts } from "./i18n";
import "./style.css";

await loadTexts();
const target = document.getElementById("app");
// One page for both windows: the label of the window picks what it shows.
if (target) mount(getCurrentWindow().label === "main" ? MainWindow : App, { target });
