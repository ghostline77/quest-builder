import { createRoot } from "react-dom/client";
import { QuestBuilder } from "./App";
import "./style.css";
const root = document.getElementById("root");
if (!root) throw Error("Missing application root.");
createRoot(root).render(<QuestBuilder />);
