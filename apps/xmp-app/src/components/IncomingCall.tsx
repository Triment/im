import { Phone, PhoneOff, Video } from "lucide-react";
import { Xmp } from "../store";
import { Avatar } from "./Avatar";

export function IncomingCall({ xmp }: { xmp: Xmp }) {
  const inc = xmp.incoming!;
  const peer = xmp.friends.find((f) => f.id === inc.peer);
  const name = peer?.display_name ?? `用户 ${inc.peer}`;
  return (
    <div className="absolute inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm animate-fade">
      <div className="w-[340px] bg-surface animate-pop rounded-[28px] border border-white/10 p-8 text-center shadow-2xl">
        <div className="relative mx-auto mb-5 w-fit">
          <span className="absolute inset-0 animate-ring rounded-full bg-brand/40" />
          <span className="absolute inset-0 animate-ring rounded-full bg-brand/40 [animation-delay:.9s]" />
          <Avatar name={name} size={96} />
        </div>
        <div className="text-xl font-semibold">{name}</div>
        <div className="mt-1 flex items-center justify-center gap-1.5 text-sm text-dim">
          {inc.video ? <Video className="h-4 w-4" /> : <Phone className="h-4 w-4" />}邀请你{inc.video ? "视频" : "语音"}通话
        </div>
        <div className="mt-7 flex justify-center gap-10">
          <button onClick={() => void xmp.rejectIncoming()} className="flex flex-col items-center gap-2 text-xs text-dim">
            <span className="flex h-16 w-16 items-center justify-center rounded-full bg-bad text-white shadow-lg shadow-bad/40 transition active:scale-90"><PhoneOff className="h-7 w-7" /></span>拒绝
          </button>
          <button onClick={() => void xmp.acceptIncoming()} className="flex flex-col items-center gap-2 text-xs text-dim">
            <span className="flex h-16 w-16 items-center justify-center rounded-full bg-good text-white shadow-lg shadow-good/40 transition active:scale-90"><Phone className="h-7 w-7" /></span>接听
          </button>
        </div>
      </div>
    </div>
  );
}
