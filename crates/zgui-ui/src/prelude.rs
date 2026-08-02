//! Every component and its props, in one import.
//!
//! A component written with `view!` needs both the component and the props type its
//! `#[component]` attribute generated, because the macro names the second to build the first.
//! Importing them in pairs by hand is a paper cut per component, so they are exported together:
//!
//! ```
//! use zgui_ui::prelude::*;
//! ```
//!
//! Nothing here is exclusive to it — every name is reachable at its own path too. The style-sheet
//! types are deliberately absent: a program that overrides one names it, and importing eighteen
//! of them into an application's own namespace would shadow more than it helps.

// The one name from the headless half an application writes by hand: it is what a `checked=` or
// `value=` prop takes, and every component here shares it.
pub use zgui_ui_primitives::Binding;

pub use crate::accordion::{
    Accordion, AccordionContent, AccordionContentProps, AccordionContext, AccordionItem,
    AccordionItemProps, AccordionProps, AccordionSelection, AccordionTrigger,
    AccordionTriggerProps,
};
pub use crate::alert::{
    Alert, AlertDescription, AlertDescriptionProps, AlertProps, AlertTitle, AlertTitleProps,
    AlertVariant,
};
pub use crate::alert_dialog::{
    AlertDialog, AlertDialogAction, AlertDialogActionProps, AlertDialogCancel,
    AlertDialogCancelProps, AlertDialogContent, AlertDialogContentProps, AlertDialogDescription,
    AlertDialogDescriptionProps, AlertDialogFooter, AlertDialogFooterProps, AlertDialogHeader,
    AlertDialogHeaderProps, AlertDialogMedia, AlertDialogMediaProps, AlertDialogProps,
    AlertDialogSize, AlertDialogTitle, AlertDialogTitleProps, AlertDialogTrigger,
    AlertDialogTriggerProps,
};
pub use crate::avatar::{
    Avatar, AvatarBadge, AvatarBadgeProps, AvatarFallback, AvatarFallbackProps, AvatarGroup,
    AvatarGroupCount, AvatarGroupCountProps, AvatarGroupProps, AvatarImage, AvatarImageProps,
    AvatarProps, AvatarSize,
};
pub use crate::badge::{Badge, BadgeProps, BadgeVariant};
pub use crate::breadcrumb::{
    Breadcrumb, BreadcrumbEllipsis, BreadcrumbEllipsisProps, BreadcrumbItem, BreadcrumbItemProps,
    BreadcrumbLink, BreadcrumbLinkProps, BreadcrumbList, BreadcrumbListProps, BreadcrumbPage,
    BreadcrumbPageProps, BreadcrumbProps, BreadcrumbSeparator, BreadcrumbSeparatorProps,
};
pub use crate::button::{Button, ButtonProps, ButtonSize, ButtonVariant};
pub use crate::calendar::{
    Calendar, CalendarDay, CalendarDayProps, CalendarMode, CalendarProps, Date, DateFilter,
    DateRange, Locale, MonthGrid, RangePlace, Weekday,
};
pub use crate::card::{
    Card, CardAction, CardActionProps, CardContent, CardContentProps, CardDescription,
    CardDescriptionProps, CardFooter, CardFooterProps, CardHeader, CardHeaderProps, CardProps,
    CardTitle, CardTitleProps,
};
pub use crate::carousel::{
    Carousel, CarouselContent, CarouselContentProps, CarouselContext, CarouselItem,
    CarouselItemProps, CarouselNext, CarouselNextProps, CarouselPrevious, CarouselPreviousProps,
    CarouselProps,
};
pub use crate::chart::{
    Chart, ChartContainer, ChartContainerProps, ChartEntry, ChartIndicator, ChartKind,
    ChartLegendContent, ChartLegendContentProps, ChartProps, ChartTooltipContent,
    ChartTooltipContentProps, Datum, LegendAlign, LegendEntry, Plot, Scale, Series,
};
pub use crate::checkbox::{Checkbox, CheckboxProps, Checked};
pub use crate::collapsible::{
    Collapsible, CollapsibleContent, CollapsibleContentProps, CollapsibleContext, CollapsibleProps,
    CollapsibleTrigger, CollapsibleTriggerProps,
};
pub use crate::combobox::{
    Combobox, ComboboxContent, ComboboxContentProps, ComboboxEmpty, ComboboxEmptyProps,
    ComboboxGroup, ComboboxGroupProps, ComboboxInput, ComboboxInputProps, ComboboxItem,
    ComboboxItemProps, ComboboxLabel, ComboboxLabelProps, ComboboxProps, ComboboxSeparator,
    ComboboxSeparatorProps,
};
pub use crate::command::{
    Command, CommandDialog, CommandDialogProps, CommandEmpty, CommandEmptyProps, CommandGroup,
    CommandGroupProps, CommandInput, CommandInputProps, CommandItem, CommandItemProps, CommandList,
    CommandListProps, CommandProps, CommandSeparator, CommandSeparatorProps, CommandShortcut,
    CommandShortcutProps,
};
pub use crate::context_menu::{
    ContextMenu, ContextMenuContent, ContextMenuContentProps, ContextMenuProps, ContextMenuTrigger,
    ContextMenuTriggerProps,
};
pub use crate::data_table::{
    Column, ColumnResizer, ColumnResizerProps, DataModel, DataTable, DataTableProps, Page,
    RowMatch, SortState,
};
pub use crate::date_picker::{DatePicker, DatePickerProps};
pub use crate::dialog::{
    Dialog, DialogClose, DialogCloseProps, DialogContent, DialogContentProps, DialogDescription,
    DialogDescriptionProps, DialogDismiss, DialogDismissProps, DialogFooter, DialogFooterProps,
    DialogHeader, DialogHeaderProps, DialogProps, DialogTitle, DialogTitleProps, DialogTrigger,
    DialogTriggerProps,
};
pub use crate::drawer::{
    Drawer, DrawerClose, DrawerCloseProps, DrawerContent, DrawerContentProps, DrawerDescription,
    DrawerDescriptionProps, DrawerFooter, DrawerFooterProps, DrawerHandle, DrawerHandleProps,
    DrawerHeader, DrawerHeaderProps, DrawerProps, DrawerTitle, DrawerTitleProps, DrawerTrigger,
    DrawerTriggerProps,
};
pub use crate::dropdown_menu::{
    DropdownMenu, DropdownMenuContent, DropdownMenuContentProps, DropdownMenuProps,
    DropdownMenuTrigger, DropdownMenuTriggerProps,
};
pub use crate::form::{
    Form, FormContext, FormDescription, FormDescriptionProps, FormField, FormFieldContext,
    FormFieldProps, FormItem, FormItemProps, FormLabel, FormLabelProps, FormMessage,
    FormMessageProps, FormProps, FormSubmit, FormSubmitProps, Validator, use_form_field,
};
pub use crate::hover_card::{
    HoverCard, HoverCardContent, HoverCardContentProps, HoverCardProps, HoverCardTrigger,
    HoverCardTriggerProps,
};
pub use crate::input::{Input, InputProps};
pub use crate::input_otp::{
    InputOtp, InputOtpGroup, InputOtpGroupProps, InputOtpProps, InputOtpSeparator,
    InputOtpSeparatorProps,
};
pub use crate::label::{Label, LabelProps};
pub use crate::menu::{
    MenuCheckboxItem, MenuCheckboxItemProps, MenuGroup, MenuGroupProps, MenuItem, MenuItemProps,
    MenuLabel, MenuLabelProps, MenuRadioGroup, MenuRadioGroupProps, MenuRadioItem,
    MenuRadioItemProps, MenuSeparator, MenuSeparatorProps, MenuShortcut, MenuShortcutProps,
    MenuSub, MenuSubContent, MenuSubContentProps, MenuSubProps, MenuSubTrigger,
    MenuSubTriggerProps,
};
pub use crate::menubar::{
    Menubar, MenubarCheckboxItem, MenubarCheckboxItemProps, MenubarContent, MenubarContentProps,
    MenubarContext, MenubarGroup, MenubarGroupProps, MenubarItem, MenubarItemProps, MenubarLabel,
    MenubarLabelProps, MenubarMenu, MenubarMenuContext, MenubarMenuProps, MenubarProps,
    MenubarRadioGroup, MenubarRadioGroupProps, MenubarRadioItem, MenubarRadioItemProps,
    MenubarSeparator, MenubarSeparatorProps, MenubarShortcut, MenubarShortcutProps, MenubarTrigger,
    MenubarTriggerProps,
};
pub use crate::navigation_menu::{
    NavigationMenu, NavigationMenuContent, NavigationMenuContentProps, NavigationMenuContext,
    NavigationMenuIndicator, NavigationMenuIndicatorProps, NavigationMenuItem,
    NavigationMenuItemContext, NavigationMenuItemProps, NavigationMenuLink,
    NavigationMenuLinkProps, NavigationMenuList, NavigationMenuListProps, NavigationMenuProps,
    NavigationMenuTrigger, NavigationMenuTriggerProps,
};
pub use crate::pagination::{
    Pagination, PaginationContent, PaginationContentProps, PaginationEllipsis,
    PaginationEllipsisProps, PaginationItem, PaginationItemProps, PaginationLink,
    PaginationLinkProps, PaginationNext, PaginationNextProps, PaginationPrevious,
    PaginationPreviousProps, PaginationProps,
};
pub use crate::popover::{
    Popover, PopoverClose, PopoverCloseProps, PopoverContent, PopoverContentProps,
    PopoverDescription, PopoverDescriptionProps, PopoverHeader, PopoverHeaderProps, PopoverProps,
    PopoverTitle, PopoverTitleProps, PopoverTrigger, PopoverTriggerProps,
};
pub use crate::progress::{Progress, ProgressProps};
pub use crate::radio_group::{RadioGroup, RadioGroupItem, RadioGroupItemProps, RadioGroupProps};
pub use crate::resizable::{
    ResizableContext, ResizableHandle, ResizableHandleProps, ResizablePanel, ResizablePanelGroup,
    ResizablePanelGroupProps, ResizablePanelProps,
};
pub use crate::scroll_area::{
    ScrollArea, ScrollAreaContext, ScrollAreaProps, ScrollBar, ScrollBarProps,
};
pub use crate::select::{
    Select, SelectContent, SelectContentProps, SelectGroup, SelectGroupProps, SelectItem,
    SelectItemProps, SelectLabel, SelectLabelProps, SelectProps, SelectSeparator,
    SelectSeparatorProps, SelectTrigger, SelectTriggerProps, SelectTriggerSize, SelectValue,
    SelectValueProps,
};
pub use crate::separator::{Separator, SeparatorOrientation, SeparatorProps};
pub use crate::sheet::{
    Sheet, SheetClose, SheetCloseProps, SheetContent, SheetContentProps, SheetDescription,
    SheetDescriptionProps, SheetDismiss, SheetDismissProps, SheetFooter, SheetFooterProps,
    SheetHeader, SheetHeaderProps, SheetProps, SheetSide, SheetTitle, SheetTitleProps,
    SheetTrigger, SheetTriggerProps,
};
pub use crate::sidebar::{
    Sidebar, SidebarCollapse, SidebarContent, SidebarContentProps, SidebarContext, SidebarFooter,
    SidebarFooterProps, SidebarGroup, SidebarGroupAction, SidebarGroupActionProps,
    SidebarGroupContent, SidebarGroupContentProps, SidebarGroupLabel, SidebarGroupLabelProps,
    SidebarGroupProps, SidebarHeader, SidebarHeaderProps, SidebarInput, SidebarInputProps,
    SidebarInset, SidebarInsetProps, SidebarMenu, SidebarMenuAction, SidebarMenuActionProps,
    SidebarMenuBadge, SidebarMenuBadgeProps, SidebarMenuButton, SidebarMenuButtonProps,
    SidebarMenuItem, SidebarMenuItemProps, SidebarMenuItemState, SidebarMenuProps, SidebarMenuSize,
    SidebarMenuSkeleton, SidebarMenuSkeletonProps, SidebarMenuSub, SidebarMenuSubButton,
    SidebarMenuSubButtonProps, SidebarMenuSubItem, SidebarMenuSubItemProps, SidebarMenuSubProps,
    SidebarMenuVariant, SidebarProps, SidebarProvider, SidebarProviderProps, SidebarRail,
    SidebarRailProps, SidebarSeparator, SidebarSeparatorProps, SidebarSide, SidebarSubSize,
    SidebarTrigger, SidebarTriggerProps, SidebarVariant,
};
pub use crate::skeleton::{Skeleton, SkeletonProps};
pub use crate::slider::{Slider, SliderProps};
pub use crate::switch::{Switch, SwitchProps, SwitchSize};
pub use crate::table::{
    CellAlign, ColumnSort, Table, TableBody, TableBodyProps, TableCaption, TableCaptionProps,
    TableCell, TableCellProps, TableFooter, TableFooterProps, TableHead, TableHeadProps,
    TableHeader, TableHeaderProps, TableProps, TableRow, TableRowProps,
};
pub use crate::tabs::{
    Tabs, TabsActivation, TabsContent, TabsContentProps, TabsContext, TabsList, TabsListProps,
    TabsListVariant, TabsProps, TabsTrigger, TabsTriggerProps,
};
pub use crate::textarea::{Textarea, TextareaProps};
pub use crate::toast::{
    Toast, ToastAction, ToastCorner, ToastItem, ToastItemProps, ToastKind, ToastQueue, Toaster,
    ToasterProps, use_toaster,
};
pub use crate::toggle::{Toggle, ToggleProps, ToggleSize, ToggleVariant};
pub use crate::tooltip::{
    Tooltip, TooltipArrow, TooltipArrowProps, TooltipContent, TooltipContentProps, TooltipDelays,
    TooltipProps, TooltipProvider, TooltipProviderProps, TooltipTrigger, TooltipTriggerProps,
};
pub use crate::virtualize::{VirtualList, VirtualListProps, VirtualWindow, Virtualize};

pub use crate::aspect_ratio::{AspectRatio, AspectRatioProps};
pub use crate::button_group::{
    ButtonGroup, ButtonGroupOrientation, ButtonGroupProps, ButtonGroupSeparator,
    ButtonGroupSeparatorProps, ButtonGroupText, ButtonGroupTextProps,
};
pub use crate::empty::{
    Empty, EmptyContent, EmptyContentProps, EmptyDescription, EmptyDescriptionProps, EmptyHeader,
    EmptyHeaderProps, EmptyMedia, EmptyMediaProps, EmptyMediaVariant, EmptyProps, EmptyTitle,
    EmptyTitleProps,
};
pub use crate::field::{
    Field, FieldContent, FieldContentProps, FieldDescription, FieldDescriptionProps, FieldError,
    FieldErrorProps, FieldGroup, FieldGroupProps, FieldLabel, FieldLabelProps, FieldLegend,
    FieldLegendProps, FieldLegendVariant, FieldOrientation, FieldProps, FieldSeparator,
    FieldSeparatorProps, FieldSet, FieldSetProps, FieldTitle, FieldTitleProps,
};
pub use crate::input_group::{
    InputGroup, InputGroupAddon, InputGroupAddonAlign, InputGroupAddonProps, InputGroupButton,
    InputGroupButtonProps, InputGroupButtonSize, InputGroupInput, InputGroupInputProps,
    InputGroupProps, InputGroupText, InputGroupTextProps, InputGroupTextarea,
    InputGroupTextareaProps,
};
pub use crate::item::{
    Item, ItemActions, ItemActionsProps, ItemContent, ItemContentProps, ItemDescription,
    ItemDescriptionProps, ItemFooter, ItemFooterProps, ItemGroup, ItemGroupProps, ItemHeader,
    ItemHeaderProps, ItemMedia, ItemMediaProps, ItemMediaVariant, ItemProps, ItemSeparator,
    ItemSeparatorProps, ItemSize, ItemTitle, ItemTitleProps, ItemVariant,
};
pub use crate::kbd::{Kbd, KbdGroup, KbdGroupProps, KbdProps};
pub use crate::native_select::{
    NativeSelect, NativeSelectOptGroup, NativeSelectOptGroupProps, NativeSelectOption,
    NativeSelectOptionProps, NativeSelectProps, NativeSelectSize,
};
pub use crate::spinner::{Spinner, SpinnerProps};
pub use crate::toggle_group::{
    ToggleGroup, ToggleGroupItem, ToggleGroupItemProps, ToggleGroupProps, ToggleSelection,
};
