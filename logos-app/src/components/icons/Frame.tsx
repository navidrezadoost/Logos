import React from "react";
import type { SVGProps } from "react";

export interface FrameProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const Frame = React.forwardRef<SVGSVGElement, FrameProps>(
  ({ size, className, style, ...props }, ref) => {
    const dimensions = size ? { width: size, height: size } : {
      width: props.width || 24,
      height: props.height || 24
    };

    return (
      <svg
        ref={ref}
        viewBox="0 0 640 640"
        xmlns="http://www.w3.org/2000/svg"
        width={dimensions.width}
        height={dimensions.height}
        fill={props.fill || "currentColor"}
        className={className}
        style={style}
        {...props}
      >
        <path fill="currentColor" d="M576 192L576 128L512 128L512 64L448 64L448 128L192 128L192 64L128 64L128 128L64 128L64 192L128 192L128 448L64 448L64 512L128 512L128 576L192 576L192 512L448 512L448 576L512 576L512 512L576 512L576 448L512 448L512 192L576 192zM192 448L192 192L448 192L448 448L192 448z"/>
      </svg>
    );
  }
);

Frame.displayName = "Frame";

export default Frame;
