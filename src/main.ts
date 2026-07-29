import "./app.css";
import { mount } from "svelte";
import App from "./components/App.svelte";

const target = document.getElementById("app");
if (!target) {
  throw new Error("#app mount point is missing from index.html");
}

export default mount(App, { target });
