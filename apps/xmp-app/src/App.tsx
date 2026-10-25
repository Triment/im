import { useXmp } from "./store";
import { Login } from "./components/Login";
import { Sidebar } from "./components/Sidebar";
import { ChatView } from "./components/ChatView";
import { CallView } from "./components/CallView";
import { IncomingCall } from "./components/IncomingCall";
import { Toasts } from "./components/Toasts";

export default function App() {
  const xmp = useXmp();
  return (
    <div className="relative h-full w-full overflow-hidden">
      {xmp.me ? (
        <div className="flex h-full">
          <Sidebar xmp={xmp} />
          <ChatView xmp={xmp} />
        </div>
      ) : (
        <Login xmp={xmp} />
      )}
      {xmp.call && <CallView xmp={xmp} />}
      {xmp.incoming && !xmp.call && <IncomingCall xmp={xmp} />}
      <Toasts toasts={xmp.toasts} />
    </div>
  );
}
