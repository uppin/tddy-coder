import { afterEach, describe, expect, test } from "bun:test";
import { act, cleanup, fireEvent, render, renderHook } from "@testing-library/react";
import { useRef, type ReactNode } from "react";
import { aMediaDouble, aPresenceDouble } from "../testkit";
import { MediaPortProvider, type MediaPort, useMediaPort } from "./media";

afterEach(cleanup);

function inMedia(port: MediaPort) {
  return ({ children }: { children: ReactNode }) => <MediaPortProvider port={port}>{children}</MediaPortProvider>;
}

const ROOM = aPresenceDouble().room("session-room");

describe("the media port", () => {
  test("useHasCameraVideo follows a participant's camera turning on", () => {
    // Given
    const media = aMediaDouble();
    const { result } = renderHook(() => useMediaPort().useHasCameraVideo(ROOM, "browser-alice"), { wrapper: inMedia(media) });

    // When
    act(() => media.setCameraVideo(ROOM, "browser-alice", true));

    // Then
    expect(result.current).toBe(true);
  });

  test("usePresenterRoom reports the room the hint names", () => {
    // Given
    const media = aMediaDouble();
    media.presentRoom("presenter-1", { room: ROOM, status: "connected" });

    // When
    const { result } = renderHook(
      () => useMediaPort().usePresenterRoom({ roomName: "presenter-1" } as never),
      { wrapper: inMedia(media) },
    );

    // Then
    expect(result.current).toEqual({ room: ROOM, status: "connected", error: null });
  });

  test("pointer input on the shared picture is forwarded through the port", () => {
    // Given
    const media = aMediaDouble();
    function Picture() {
      const picture = useRef<HTMLDivElement>(null);
      useMediaPort().useScreenSharingInput({
        picture,
        room: ROOM,
        bridgeIdentity: "screen-bridge",
        framebufferWidth: 100,
        framebufferHeight: 100,
        onCloseChord: () => {},
      });
      return <div data-testid="picture" ref={picture} style={{ width: 100, height: 100 }} />;
    }
    const { getByTestId } = render(<Picture />, { wrapper: inMedia(media) });

    // When
    fireEvent.pointerDown(getByTestId("picture"), { clientX: 10, clientY: 20, buttons: 1 });

    // Then
    expect(media.forwardedInput.map((i) => i.kind)).toEqual(["pointer"]);
  });

  test("RemoteVideo renders the port's video for the participant", () => {
    // Given
    const media = aMediaDouble();
    function Tile() {
      const { RemoteVideo } = useMediaPort();
      return <RemoteVideo room={ROOM} identity="browser-alice" trackName="camera" />;
    }

    // When
    const { getByTestId } = render(<Tile />, { wrapper: inMedia(media) });

    // Then
    expect(getByTestId("remote-video-browser-alice-camera")).toBeTruthy();
  });
});
