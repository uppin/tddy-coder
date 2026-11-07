/** A single terminal I/O event, as fired by the terminal's `onBytes` sink. One direction per event:
 *  an output chunk carries `bytesIn`, a batched input yield carries `bytesOut`. Either field omitted
 *  ⇒ 0. */
export interface ByteDelta {
  bytesIn?: number;
  bytesOut?: number;
}
