import { Router, Route } from '@solidjs/router';
import Chat from './pages/Chat';
import Composer from './pages/Composer';

export default function App() {
  return (
    <Router>
      <Route path="/" component={Chat} />
      <Route path="/composer" component={Composer} />
    </Router>
  );
}
