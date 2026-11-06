# Text personalization

Select one unlocked text object in Design, then choose **Tools > Personalize Text...**
or **Personalize Text...** in its text properties. These controls are available
in builds containing the personalization dialog. Older releases expose only
the underlying API.

The dialog keeps edits local until you choose Apply or Create copies. Cancel
leaves the project unchanged. Changing the open project closes the dialog.

## Serial numbers

Choose Serial Number. Set Start, Advance By, Serial Padding, and Copies. For
example, `SN-{Serial}` with start 100, increment 2, padding 4, and three copies
previews `SN-0100`, `SN-0102`, and `SN-0104`. Prefixes, suffixes, and line breaks
are ordinary template text. Negative increments are supported; zero is rejected.

## Dates and times

Choose Date/Time and select a date format. Templates can include ordinary text
around a date field, such as `Made {Date:YYYY-MM-DD}`. The supported date tokens
are `YYYY`, `YY`, `MM`, `DD`, `HH`, `mm`, and `ss`.

Dates use the computer's local time. The preview captures a timestamp so generated
copies match the reviewed values, including seconds. Refresh date/time captures
a new timestamp. Applying a date configuration to the original keeps it live;
generated copies keep their reviewed dates as ordinary editable text.

## CSV personalization

Choose Merge/CSV and load a UTF-8 CSV with a header row and at least one data row.
Headers must be nonempty and unique. Quoted commas, escaped quotes, multiline
cells, Unicode, and a UTF-8 byte-order mark are supported. Malformed quoting or
inconsistent column counts produce an error.

The first column is inserted automatically. Click column buttons to insert more
fields, then edit any separators or surrounding text. For example,
`{CSV:Name} / {CSV:Code}` can produce `Zoë / A1` and `Jo / A2`.
Column names are case-sensitive. First data row is numbered from 1, excluding
the header. Choose the number of copies to include. The dialog rejects a range
that would run past the available rows instead of silently repeating names.
The CSV data is embedded in the saved project, so reopening it does not require
the original CSV file.

## Apply, create, and undo

Apply saves the configuration to the selected text object. Create copies makes
1 to 1000 independent editable text objects using the previewed values. Horizontal
and vertical spacing use the current display units. The original remains as a
reusable template, with its next serial number or CSV row advanced for the next
batch. Generated copies keep the source's font, style, layer, and power scale.

One Undo removes the entire batch and restores the source settings. Redo restores
the batch. Saving and reopening preserves the template, embedded CSV, and generated
text values. The original remains in the project: select the generated artwork
when you want to preview or run just those copies.

To return a configured object to ordinary text, choose Normal and Apply. This
keeps the first previewed value and clears personalization. Editing its content
in Text Properties or on the canvas also clears its personalization settings.
