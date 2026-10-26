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
          <div className={`${xmp.selected ? "hidden sm:flex" : "flex"} h-full w-full sm:w-[320px] shrink-0`}>
            <Sidebar xmp={xmp} />
          </div>
          <div className={`${xmp.selected ? "flex" : "hidden sm:flex"} h-full min-w-0 flex-1`}>
            <ChatView xmp={xmp} />
          </div>
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
