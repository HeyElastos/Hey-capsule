package os.elastos.hey.social

import com.journeyapps.barcodescanner.CaptureActivity

/**
 * QR capture screen that follows the phone normally (portrait) instead of the
 * zxing default, which forces landscape. Wired via ScanOptions.setCaptureActivity;
 * the portrait lock itself lives on this activity's manifest entry (sensorPortrait).
 */
class PortraitCaptureActivity : CaptureActivity()
