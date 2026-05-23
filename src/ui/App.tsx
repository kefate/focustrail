import { FloatingTimer } from "./FloatingTimer";
import { MainWindow } from "./MainWindow";
import { RestOverlay } from "./RestOverlay";

export function App() {
  if (window.location.hash === "#floating") {
    return <FloatingTimer />;
  }

  if (window.location.hash === "#rest-overlay") {
    return <RestOverlay />;
  }

  return <MainWindow />;
}
