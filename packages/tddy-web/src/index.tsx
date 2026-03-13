import { createRoot } from "react-dom/client";
import { Button } from "./components/Button";

function App() {
  return (
    <div>
      <h1>tddy-web</h1>
      <Button label="Dashboard" onClick={() => {}} />
    </div>
  );
}

const root = document.getElementById("root");
if (root) {
  createRoot(root).render(<App />);
}
