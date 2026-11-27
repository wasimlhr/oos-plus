Fixed:

- The VNC page and desktop VNC viewers follow the screen at the rate the scope draws it, not at a few updates a
  second: screens are made ahead of the viewer's next request, a viewer that offers ContinuousUpdates gets them with
  no request each, desktop viewers are sent ZRLE instead of whole raw screens (a few KB an update instead of
  hundreds), and a viewer's thread is no longer starved by the rest of the scope
  ([HTTP reference](docs/http.md)).
