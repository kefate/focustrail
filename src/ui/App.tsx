import { FloatingTimer } from "./FloatingTimer";
import { MainWindow } from "./MainWindow";

export function App() {
  const isFloatingWindow = window.location.hash === "#floating";
  return isFloatingWindow ? <FloatingTimer /> : <MainWindow />;
}
