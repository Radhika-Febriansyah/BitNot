import { t } from "../i18n";
import type { Sort, SortField } from "../lib/library";
import { useUi } from "../state/UiContext";
import { Icon } from "./Icon";

interface SortButtonProps {
  sort: Sort;
  onSort: (field: SortField) => void;
  /** Fields offered besides the default order. */
  fields?: SortField[];
}

const ALL_FIELDS: SortField[] = ["title", "artist", "album", "duration", "added"];

/** Opens a menu of sort orders. Choosing the active field again reverses it. */
export function SortButton({ sort, onSort, fields = ALL_FIELDS }: SortButtonProps) {
  const { openMenu } = useUi();
  const direction = t(sort.descending ? "sort.descending" : "sort.ascending");
  const label =
    sort.field === "default"
      ? t("sort.button")
      : `${t("sort.sortedBy", { field: t(`sort.${sort.field}`) })}, ${direction}`;
  return (
    <button
      className={sort.field === "default" ? "icon-btn" : "icon-btn on"}
      aria-label={label}
      aria-haspopup="menu"
      title={label}
      onClick={(event) => {
        const box = event.currentTarget.getBoundingClientRect();
        openMenu(
          [
            { label: t("sort.default"), onSelect: () => onSort("default") },
            { separator: true },
            ...fields.map((field) => ({
              label:
                sort.field === field ? `${t(`sort.${field}`)} (${direction})` : t(`sort.${field}`),
              onSelect: () => onSort(field),
            })),
          ],
          box.left,
          box.bottom + 4,
        );
      }}
    >
      <Icon name="sort" size={1.25} />
    </button>
  );
}
