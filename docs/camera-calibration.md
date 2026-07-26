# Camera calibration

Beam Bench can align a fixed overhead camera to the laser workspace and correct
the perspective of an angled view. The recommended nine-point setup also
corrects the curved edges caused by wide-angle lenses.

## Before you start

- Mount the camera securely where it can see the full usable workspace.
- Set the camera focus and keep the same capture resolution you intend to use.
- Place clearly visible marks at known workspace coordinates. For the
  nine-point setup, distribute the marks across the bed in a 3 by 3 grid. Keep
  them away from the exact edge so they are easy to see and select.
- Do not move the camera, bed, or mount after calibration.

## Calibrate the camera

1. Open the **Camera** panel and select the camera.
2. Select **Update Overlay** to capture a current image.
3. Select **Align Camera**.
4. Choose **Wide-angle setup (9 points, recommended)**. Use the four-point
   quick setup only when the lens has very little visible distortion.
5. Select each numbered point. Enter the known workspace X and Y coordinates
   of its physical mark, then click that same mark in the camera image.
6. Select **Solve**. A lower RMSE value means the selected points agree more
   closely.
7. Select **Save Alignment**.
8. Check the result against a few known positions across the bed before relying
   on the overlay for a job. Small final placement changes can be made with the
   overlay adjustment controls.

## If the overlay is still inaccurate

- Repeat the setup with points spread across more of the visible bed.
- Make sure every image point was paired with the correct workspace
  coordinate.
- Improve lighting and use smaller, sharper reference marks.
- Recalibrate after changing the camera position, focus, mount, or capture
  resolution.
- If the center is accurate but the edges curve away, use the nine-point setup.

Calibration affects the camera overlay only. It does not change machine steps,
homing, job coordinates, or laser motion.
