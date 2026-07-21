export interface SessionJobOptions {
  cut_selected_graphics: boolean;
  use_selection_origin: boolean;
  selected_object_ids: string[];
}

export function sessionJobOptions(
  options: {
    cut_selected_graphics: boolean;
    use_selection_origin: boolean;
  },
  selectedObjectIds: string[],
): SessionJobOptions {
  return {
    cut_selected_graphics: options.cut_selected_graphics,
    use_selection_origin: options.use_selection_origin,
    selected_object_ids:
      options.cut_selected_graphics || options.use_selection_origin ? selectedObjectIds : [],
  };
}
